use crate::configure::config::MutePorter;
use crate::configure::Config;
use crate::datastructures::notifies::ClientBasicInfo;
use crate::datastructures::QueryResult;
use crate::observer::PrivateMessageRequest;
use crate::socketlib::SocketConn;
use crate::{AUTO_CHANNEL_NICKNAME_OVERRIDE, DEFAULT_AUTO_CHANNEL_NICKNAME};
use anyhow::anyhow;
use log::{debug, error, info, trace, warn};
use redis::AsyncCommands;
use std::time::Duration;
use tap::{Tap, TapFallible};
use tokio::sync::mpsc;

pub enum AutoChannelEvent {
    Update(ClientBasicInfo),
    DeleteChannel(i64, String),
    Terminate,
}

#[derive(Clone, Debug)]
pub struct AutoChannelInstance {
    channel_ids: Vec<i64>,
    sender: Option<mpsc::Sender<AutoChannelEvent>>,
}

impl AutoChannelInstance {
    pub async fn send_terminate(&self) -> anyhow::Result<()> {
        match self.sender {
            Some(ref sender) => sender
                .send(AutoChannelEvent::Terminate)
                .await
                .map_err(|_| anyhow!("Got error while send terminate to auto channel staff")),
            None => Ok(()),
        }
    }

    // TODO: Optimize
    async fn send_signal(&self, signal: AutoChannelEvent) -> anyhow::Result<bool> {
        match self.sender {
            Some(ref sender) => sender
                .send(signal)
                .await
                .map_err(|_| anyhow!("Got error while send event to auto channel staff"))
                .map(|_| true),
            _ => Ok(false),
        }
    }

    pub async fn send_delete(&self, user_id: i64, uid: String) -> anyhow::Result<bool> {
        self.send_signal(AutoChannelEvent::DeleteChannel(user_id, uid))
            .await
    }

    pub async fn send(&self, view: ClientBasicInfo) -> anyhow::Result<bool> {
        if self.sender.is_none() {
            return Ok(false);
        }
        if !self.channel_ids.iter().any(|id| id == &view.channel_id()) {
            return Ok(false);
        }
        self.send_signal(AutoChannelEvent::Update(view)).await
    }

    pub fn new(channel_ids: Vec<i64>, sender: Option<mpsc::Sender<AutoChannelEvent>>) -> Self {
        Self {
            channel_ids,
            sender,
        }
    }

    pub fn valid(&self) -> bool {
        self.sender.is_some()
    }
}

pub async fn mute_porter_function(
    conn: &mut SocketConn,
    mute_porter: &MutePorter,
) -> QueryResult<()> {
    for client in conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("Unable query clients: {:?}", e))?
    {
        if client.client_is_user()
            && client.channel_id() == mute_porter.monitor_channel()
            && !mute_porter.check_whitelist(client.client_database_id())
        {
            if let Some(true) = conn
                .query_client_info(client.client_id())
                .await
                .tap_err(|e| error!("Unable query client information: {:?}", e))
                .ok()
                .flatten()
                .map(|r| r.is_client_muted())
            {
                conn.move_client(client.client_id(), mute_porter.target_channel())
                    .await
                    .tap_err(|e| {
                        error!(
                            "Unable move client {} to channel {}: {:?}",
                            client.client_id(),
                            mute_porter.target_channel(),
                            e
                        )
                    })
                    .map(|_| {
                        info!(
                            "Moved {} to {}",
                            client.client_id(),
                            mute_porter.target_channel()
                        )
                    })
                    .ok();
            }
        }
    }
    Ok(())
}

fn build_redis_key(client_database_id: i64, server_id: &str, channel_id: i64) -> String {
    format!(
        "ts_autochannel_{}_{server_id}_{pid}",
        client_database_id,
        server_id = server_id,
        pid = channel_id
    )
}

pub async fn auto_channel_staff(
    mut conn: SocketConn,
    mut receiver: mpsc::Receiver<AutoChannelEvent>,
    private_message_sender: mpsc::Sender<PrivateMessageRequest>,
    config: Config,
) -> anyhow::Result<()> {
    let redis = redis::Client::open(config.server().redis_server())
        .map_err(|e| anyhow!("Connect redis server error! {:?}", e))?;
    let mut redis_conn = redis
        .get_async_connection()
        .await
        .map_err(|e| anyhow!("Get redis connection error: {:?}", e))?;

    let monitor_channels = config.server().channels();
    let privilege_group = config.server().privilege_group_id();
    let channel_permissions = config.channel_permissions();
    let moved_message = config.message().move_to_channel();
    conn.change_nickname(
        AUTO_CHANNEL_NICKNAME_OVERRIDE.get_or_init(|| DEFAULT_AUTO_CHANNEL_NICKNAME.to_string()),
    )
    .await
    .map_err(|e| anyhow!("Got error while change nickname: {:?}", e))?;

    let who_am_i = conn
        .who_am_i()
        .await
        .map_err(|e| anyhow!("Whoami failed: {:?}", e))?;

    let server_info = conn
        .query_server_info()
        .await
        .map_err(|e| anyhow!("Query server info error: {:?}", e))?;

    info!("Connected: {}", who_am_i.client_id());
    debug!("Monitor: {}", monitor_channels.len());

    let mut skip_sleep = true;
    loop {
        if !skip_sleep {
            //std::thread::sleep(Duration::from_millis(interval));
            match tokio::time::timeout(Duration::from_secs(30), receiver.recv()).await {
                Ok(Some(event)) => match event {
                    AutoChannelEvent::Terminate => break,
                    AutoChannelEvent::Update(view) => {
                        if view.client_id() == who_am_i.client_id() {
                            continue;
                        }
                    }
                    AutoChannelEvent::DeleteChannel(client_id, uid) => {
                        let result = conn
                            .client_get_database_id_from_uid(&uid)
                            .await
                            .map_err(|e| anyhow!("Got error while query {} {:?}", uid, e))?;
                        for channel_id in &monitor_channels {
                            let key = build_redis_key(
                                result.client_database_id(),
                                server_info.virtual_server_unique_identifier(),
                                *channel_id,
                            );

                            redis_conn
                                .del::<_, i64>(&key)
                                .await
                                .tap(|_| trace!("Deleted"))
                                .tap_err(|e| error!("Got error while delete from redis: {:?}", e))
                                .ok();
                        }
                        private_message_sender
                            .send(PrivateMessageRequest::Message(
                                client_id,
                                "Received.".into(),
                            ))
                            .await
                            .tap_err(|_| error!("Got error in request send message"))
                            .ok();
                    }
                },
                Ok(None) => {
                    error!("Channel closed!");
                    break;
                }
                Err(_) => {
                    conn.who_am_i()
                        .await
                        .map_err(|e| anyhow!("Got error while doing keep alive {:?}", e))
                        .ok();
                    if config.mute_porter().enable() {
                        mute_porter_function(&mut conn, config.mute_porter()).await?;
                    }
                    continue;
                }
            }
        } else {
            skip_sleep = false;
        }
        let clients = match conn
            .query_clients()
            .await
            .tap_err(|e| error!("Got error while query clients: {:?}", e))
        {
            Ok(clients) => clients,
            Err(_) => continue,
        };

        'outer: for client in clients {
            if client.client_database_id() == who_am_i.client_database_id()
                || !monitor_channels.iter().any(|v| *v == client.channel_id())
                || client.client_type() == 1
            {
                continue;
            }
            let key = format!(
                "ts_autochannel_{}_{server_id}_{pid}",
                client.client_database_id(),
                server_id = server_info.virtual_server_unique_identifier(),
                pid = client.channel_id()
            );

            let ret: Option<i64> = redis_conn.get(&key).await?;
            let create_new = ret.is_none();
            let target_channel = if create_new {
                let mut name = format!("{}'s channel", client.client_nickname());
                let channel_id = loop {
                    let create_channel = match conn.create_channel(&name, client.channel_id()).await
                    {
                        Ok(ret) => ret,
                        Err(e) => {
                            if e.code() == 771 {
                                name.push('1');
                                continue;
                            }
                            error!("Got error while create {:?} channel: {:?}", name, e);
                            continue 'outer;
                        }
                    };

                    break create_channel.unwrap().cid();
                };

                conn.set_client_channel_group(
                    client.client_database_id(),
                    channel_id,
                    privilege_group,
                )
                .await
                .tap_err(|e| error!("Got error while set client channel group: {:?}", e))
                .ok();

                conn.add_channel_permission(channel_id, &[(133, 75)])
                    .await
                    .tap_err(|e| error!("Got error while set default channel permissions: {:?}", e))
                    .ok();

                if let Some(permissions) = channel_permissions.get(&client.channel_id()) {
                    conn.add_channel_permission(channel_id, permissions)
                        .await
                        .tap_err(|e| error!("Got error while set channel permissions: {:?}", e))
                        .ok();
                }

                channel_id
            } else {
                ret.unwrap()
            };

            match conn.move_client(client.client_id(), target_channel).await {
                Ok(ret) => ret,
                Err(e) => {
                    if e.code() == 768 {
                        redis_conn.del(&key).await?;
                        skip_sleep = true;
                        continue;
                    }
                    error!("Got error while move client: {:?}", e);
                    continue;
                }
            };

            private_message_sender
                .send(PrivateMessageRequest::Message(
                    client.client_id(),
                    moved_message.clone().into(),
                ))
                .await
                .tap_err(|_| warn!("Send message request fail"))
                .ok();

            if create_new {
                conn.move_client(who_am_i.client_id(), client.channel_id())
                    .await
                    .map_err(|e| anyhow!("Unable move self out of channel. {:?}", e))?;
                redis_conn.set(&key, target_channel).await?;
            }

            info!("Move {} to {}", client.client_nickname(), target_channel);
        }
    }
    conn.logout().await?;
    Ok(())
}
