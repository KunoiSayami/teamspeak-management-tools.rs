use crate::datastructures::NotifyClientMovedView;
use crate::socketlib::SocketConn;
use anyhow::anyhow;
use log::{error, info};
use once_cell::sync::OnceCell;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::hint::unreachable_unchecked;
use std::time::Duration;
use tokio::sync::mpsc;

pub static MSG_CHANNEL_NOT_FOUND: OnceCell<String> = OnceCell::new();
pub static MSG_CREATE_CHANNEL: OnceCell<String> = OnceCell::new();
pub static MSG_MOVE_TO_CHANNEL: OnceCell<String> = OnceCell::new();

pub enum AutoChannelEvent {
    Update(NotifyClientMovedView),
    Terminate,
}

#[derive(Clone, Debug)]
pub struct AutoChannelInstance {
    channel_ids: Vec<i64>,
    sender: Option<mpsc::Sender<AutoChannelEvent>>,
}

impl AutoChannelInstance {
    pub fn channel_ids(&self) -> &Vec<i64> {
        &self.channel_ids
    }

    pub async fn send_terminate(&self) -> anyhow::Result<()> {
        match self.sender {
            Some(ref sender) => sender
                .send(AutoChannelEvent::Terminate)
                .await
                .map_err(|_| anyhow!("Got error while send terminate to auto channel staff")),
            None => Ok(()),
        }
    }

    pub async fn send(&self, view: NotifyClientMovedView) -> anyhow::Result<bool> {
        if self.sender.is_none() {
            return Ok(false);
        }
        if !self.channel_ids.iter().any(|id| id == &view.channel_id()) {
            return Ok(false);
        }

        match self.sender {
            Some(ref sender) => sender
                .send(AutoChannelEvent::Update(view))
                .await
                .map_err(|_| anyhow!("Got error while send event to auto channel staff"))
                .map(|_| true),
            _ => unsafe { unreachable_unchecked() },
        }
    }

    pub fn new_none() -> Self {
        Self::new(vec![], None)
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

pub async fn auto_channel_staff(
    mut conn: SocketConn,
    monitor_channels: Vec<i64>,
    privilege_group: i64,
    redis_server: String,
    interval: u64,
    mut receiver: mpsc::Receiver<AutoChannelEvent>,
    channel_permissions: HashMap<i64, Vec<(u64, i64)>>,
) -> anyhow::Result<()> {
    info!(
        "Interval is: {}, version: {}",
        interval,
        env!("CARGO_PKG_VERSION")
    );

    let redis = redis::Client::open(redis_server)
        .map_err(|e| anyhow!("Connect redis server error! {:?}", e))?;
    let mut redis_conn = redis
        .get_async_connection()
        .await
        .map_err(|e| anyhow!("Get redis connection error: {:?}", e))?;

    let who_am_i = conn
        .who_am_i()
        .await
        .map_err(|e| anyhow!("Whoami failed: {:?}", e))?;

    let server_info = conn
        .query_server_info()
        .await
        .map_err(|e| anyhow!("Query server info error: {:?}", e))?;

    info!("Connected: {}", who_am_i.client_id());

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
                    continue;
                }
            }
        } else {
            skip_sleep = false;
        }
        let clients = match conn
            .query_clients()
            .await
            .map_err(|e| error!("Got error while query clients: {:?}", e))
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
                conn.send_text_message(client.client_id(), MSG_CHANNEL_NOT_FOUND.get().unwrap())
                    .await
                    .map_err(|e| error!("Got error while send message: {:?}", e))
                    .ok();

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

                    conn.send_text_message(client.client_id(), MSG_CREATE_CHANNEL.get().unwrap())
                        .await
                        .map_err(|e| error!("Got error while send message: {:?}", e))
                        .ok();

                    break create_channel.unwrap().cid();
                };

                conn.set_client_channel_group(
                    client.client_database_id(),
                    channel_id,
                    privilege_group,
                )
                .await
                .map_err(|e| error!("Got error while set client channel group: {:?}", e))
                .ok();

                conn.add_channel_permission(channel_id, &[(133, 75)])
                    .await
                    .map_err(|e| error!("Got error while set default channel permissions: {:?}", e))
                    .ok();

                if let Some(permissions) = channel_permissions.get(&client.channel_id()) {
                    conn.add_channel_permission(channel_id, permissions)
                        .await
                        .map_err(|e| error!("Got error while set channel permissions: {:?}", e))
                        .ok();
                }

                channel_id
            } else {
                ret.unwrap()
            };

            match conn
                .move_client_to_channel(client.client_id(), target_channel)
                .await
            {
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

            conn.send_text_message(client.client_id(), MSG_MOVE_TO_CHANNEL.get().unwrap())
                .await
                .map_err(|e| error!("Got error while send message: {:?}", e))
                .ok();

            if create_new {
                conn.move_client_to_channel(who_am_i.client_id(), client.channel_id())
                    .await
                    .map_err(|e| anyhow!("Unable move self out of channel. {:?}", e))?;
                //mapper.insert(client.client_database_id(), target_channel);
                redis_conn.set(&key, target_channel).await?;
            }

            info!("Move {} to {}", client.client_nickname(), target_channel);
        }
    }
    conn.logout().await?;
    Ok(())
}
