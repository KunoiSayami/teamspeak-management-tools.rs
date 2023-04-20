use crate::auto_channel::{AutoChannelEvent, AutoChannelInstance};
use crate::datastructures::config::MutePorter;
use crate::datastructures::output;
use crate::datastructures::{
    BanEntry, FromQueryString, NotifyClientEnterView, NotifyClientLeftView, NotifyClientMovedView,
    NotifyTextMessage,
};
use crate::socketlib::SocketConn;
use crate::{Config, OBSERVER_NICKNAME_OVERRIDE};
use anyhow::anyhow;
use futures_util::future::FutureExt;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::fmt::Formatter;
use std::hint::unreachable_unchecked;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex};

pub enum PrivateMessageRequest {
    Message(i64, String),
    Terminate,
}

pub enum TelegramData {
    Enter(String, i64, String, String, String),
    Left(String, NotifyClientLeftView, String),
    Terminate,
}

impl TelegramData {
    fn from_left(time: String, view: &NotifyClientLeftView, nickname: String) -> Self {
        Self::Left(time, view.clone(), nickname)
    }
    fn from_enter(time: String, view: NotifyClientEnterView) -> Self {
        Self::Enter(
            time,
            view.client_id(),
            view.client_unique_identifier().to_string(),
            view.client_nickname().to_string(),
            view.client_country().to_string(),
        )
    }
}

impl std::fmt::Display for TelegramData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TelegramData::Enter(time, client_id, client_identifier, nickname, country) => {
                write!(
                    f,
                    "[{}] <b>{}</b>(<code>{}</code>:{})[{}] joined",
                    time,
                    nickname,
                    client_identifier,
                    client_id,
                    country_emoji::flag(country).unwrap_or_else(|| country.to_string())
                )
            }
            TelegramData::Left(time, view, nickname) => match view.reason_id() {
                8 => {
                    if view.reason().is_empty() {
                        write!(
                            f,
                            "[{}] <b>{}</b>({}) left",
                            time,
                            nickname,
                            view.client_id()
                        )
                    } else {
                        write!(
                            f,
                            "[{}] <b>{}</b>({}) left ({})",
                            time,
                            nickname,
                            view.client_id(),
                            view.reason()
                        )
                    }
                }
                3 => write!(
                    f,
                    "[{}] <b>{}</b>({}) connection lost #timeout",
                    time,
                    nickname,
                    view.client_id()
                ),
                5 | 6 => {
                    write!(f,
                           "[{time}] <b>{nickname}</b>({client_id}) was #{operation} by <b>{invoker}</b>(<code>{invoker_uid}</code>){reason}",
                           time = time,
                           nickname = nickname,
                           operation = if view.reason_id() == 5 { "kicked" } else { "banned" },
                           client_id = view.client_id(),
                           invoker = view.invoker_name(),
                           invoker_uid = view.invoker_uid(),
                           reason = if view.reason().is_empty() {
                               " with no reason".to_string()
                           } else {
                               format!(": {}", view.reason())
                           }
                    )
                }
                _ => unreachable!("Got unexpected left message: {:?}", view),
            },
            TelegramData::Terminate => unsafe {
                unreachable_unchecked();
            },
        }
    }
}

pub async fn telegram_thread(
    token: String,
    target: i64,
    server: String,
    mut receiver: mpsc::Receiver<TelegramData>,
) -> anyhow::Result<()> {
    if token.is_empty() {
        warn!("Token is empty, skipped all send message request.");
        while let Some(cmd) = receiver.recv().await {
            if let TelegramData::Terminate = cmd {
                break;
            }
        }
        return Ok(());
    }
    let bot = Bot::new(token).set_api_url(server.parse()?);

    let bot = bot.parse_mode(ParseMode::Html);
    while let Some(cmd) = receiver.recv().await {
        if let TelegramData::Terminate = cmd {
            break;
        }
        let payload = bot.send_message(ChatId(target), cmd.to_string());
        if let Err(e) = payload.send().await {
            error!("Got error in send message {:?}", e);
        }
    }
    debug!("Send message daemon exiting...");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn staff(
    line: &str,
    received: &mut bool,
    ignore_list: &[String],
    monitor_channel: &AutoChannelInstance,
    whitelist_ip: &Vec<String>,
    client_map: &mut HashMap<i64, (String, bool)>,
    sender: &mpsc::Sender<TelegramData>,
    current_time: &str,
    conn: &mut SocketConn,
    output_file: Option<Arc<Mutex<File>>>,
    mute_porter: &MutePorter,
) -> anyhow::Result<()> {
    if line.starts_with("notifycliententerview") {
        let view = NotifyClientEnterView::from_query(line)
            .map_err(|e| anyhow!("Got error while deserialize enter view: {:?}", e))?;
        let is_server_query = view.client_unique_identifier().eq("ServerQuery")
            || ignore_list
                .iter()
                .any(|element| element.eq(view.client_unique_identifier()));
        client_map.insert(
            view.client_id(),
            (view.client_nickname().to_string(), is_server_query),
        );
        if is_server_query {
            return Ok(());
        }
        tokio::join!(
            monitor_channel.send(view.clone().into()).map(|result| {
                result.map(|sent| {
                    if sent {
                        trace!("Notify auto channel thread")
                    }
                })
            }),
            sender
                .send(TelegramData::from_enter(
                    current_time.to_string(),
                    view.clone()
                ))
                .map(|result| result
                    .map_err(|_| error!("Got error while send data to telegram"))
                    .ok()),
            async {
                if let Some(file) = output_file {
                    let mut file = file.lock().await;
                    file.write(
                        output::UserInChannel::new(
                            view.client_id(),
                            view.client_unique_identifier().to_string(),
                            view.channel_id(),
                        )
                        .with_new_line()
                        .as_bytes(),
                    )
                    .await
                    .map_err(|e| error!("Can't write output to file: {:?}", e))
                    .ok();
                }
            }
        )
        .0?;

        return Ok(());
    }
    if line.starts_with("notifyclientleftview") {
        let view = NotifyClientLeftView::from_query(line)
            .map_err(|e| anyhow!("Got error while deserialize left view: {:?}", e))?;
        if !client_map.contains_key(&view.client_id()) {
            warn!("Can't find client: {:?}", view.client_id());
            return Ok(());
        }
        let nickname = client_map.get(&view.client_id()).unwrap();
        if nickname.1 {
            return Ok(());
        }
        sender
            .send(TelegramData::from_left(
                current_time.to_string(),
                &view,
                nickname.0.clone(),
            ))
            .await
            .map_err(|_| error!("Got error while send data to telegram"))
            .ok();
        client_map.remove(&view.client_id());
        return Ok(());
    }
    if line.contains("notifyclientmoved") && monitor_channel.valid() {
        let view = NotifyClientMovedView::from_query(line)
            .map_err(|e| anyhow!("Got error while deserialize moved view: {:?}", e))?;
        monitor_channel
            .send(view.clone().into())
            .await
            .map(|sent| {
                if sent {
                    trace!("Notify auto channel thread")
                }
            })?;
        if let Some(file) = output_file {
            let mut file = file.lock().await;
            file.write(
                output::UserMoveToChannel::new(
                    view.client_id(),
                    view.channel_id(),
                    if view.invoker_uid().is_empty() {
                        None
                    } else {
                        Some(view.invoker_uid().to_string())
                    },
                )
                .with_new_line()
                .as_bytes(),
            )
            .await
            .map_err(|e| error!("Can't write output to file: {:?}", e))
            .ok();
        }
        return Ok(());
    }
    if line.contains("notifytextmessage") && monitor_channel.valid() {
        let view = NotifyTextMessage::from_query(line)
            .map_err(|e| anyhow!("Got error while deserialize moved view: {:?}", e))?;
        if !view.msg().eq("!reset") {
            return Ok(());
        }
        monitor_channel
            .send_signal(AutoChannelEvent::DeleteChannel(
                view.invoker_id(),
                view.invoker_uid().to_string(),
            ))
            .await
            .map(|_| {
                info!(
                    "Notify auto channel thread reset {}({})",
                    view.invoker_name(),
                    view.invoker_uid()
                )
            })?;
        return Ok(());
    }
    if line.starts_with("banid") {
        if whitelist_ip.is_empty() {
            return Ok(());
        }
        for entry in line.split('|').map(BanEntry::from_query) {
            let entry = entry?;
            if whitelist_ip.iter().any(|ip| entry.ip().eq(ip)) {
                conn.ban_del(entry.ban_id()).await.map(|_| {
                    info!(
                        "Remove whitelist ip {} from ban list (was {})",
                        entry.ip(),
                        entry
                    )
                })?
            }
        }
        return Ok(());
    }
    if line.contains("virtualserver_status=") {
        *received = true;
        if !mute_porter.enable() {
            return Ok(());
        }
        for client in conn
            .query_client_list()
            .await
            .map_err(|e| anyhow!("Unable query clients: {:?}", e))?
        {
            if client.is_client_valid()
                && client.channel_id() == mute_porter.monitor_channel()
                && !mute_porter.check_whitelist(client.client_database_id())
            {
                if let Some(true) = conn
                    .query_client_info(client.client_id())
                    .await
                    .map_err(|e| error!("Unable query client information: {:?}", e))
                    .ok()
                    .flatten()
                    .map(|r| r.is_client_muted())
                {
                    conn.move_client_to_channel(client.client_id(), mute_porter.target_channel())
                        .await
                        .map_err(|e| {
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
    }
    Ok(())
}

pub async fn observer_thread(
    mut conn: SocketConn,
    mut recv: mpsc::Receiver<PrivateMessageRequest>,
    sender: mpsc::Sender<TelegramData>,
    notify_signal: Arc<Mutex<bool>>,
    monitor_channel: AutoChannelInstance,
    config: Config,
    output_server_broadcast: Option<String>,
) -> anyhow::Result<()> {
    let interval = config.misc().interval();
    let whitelist_ip = config.server().whitelist_ip();
    let ignore_list = config.server().ignore_user_name();
    info!(
        "Version: {}, interval: {}, ban list checker: {}, mute porter: {}",
        env!("CARGO_PKG_VERSION"),
        interval,
        !whitelist_ip.is_empty(),
        config.mute_porter().enable()
    );

    let mut output_file = if let Some(ref path) = output_server_broadcast {
        let f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(path)
            .await
            .map_err(|e| anyhow!("Unable to open file {} {:?}", path, e))?;
        Some(f)
    } else {
        None
    };

    conn.change_nickname(OBSERVER_NICKNAME_OVERRIDE.get_or_init(|| "observer".to_string()))
        .await
        .map_err(|e| anyhow!("Got error while change nickname: {:?}", e))?;
    let mut client_map: HashMap<i64, (String, bool)> = HashMap::new();
    for client in conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("QueryClient failure: {:?}", e))?
    {
        if client_map.get(&client.client_id()).is_some() || client.client_type() == 1 {
            continue;
        }

        if let Some(ref mut f) = output_file {
            let unique_identifier = if let Ok(Some(id)) = conn
                .query_client_info(client.client_id())
                .await
                .map(|r| r.map(|info| info.client_unique_identifier()))
            {
                id
            } else {
                format!("{}", client.client_database_id())
            };
            f.write(
                output::UserInChannel::new(
                    client.client_id(),
                    unique_identifier,
                    client.channel_id(),
                )
                .with_new_line()
                .as_bytes(),
            )
            .await
            .map_err(|e| error!("Can't write output to file: {:?}", e))
            .ok();
        }

        client_map.insert(
            client.client_id(),
            (client.client_nickname().to_string(), false),
        );
    }

    let output_file = output_file.map(|f| Arc::new(Mutex::new(f)));

    // TODO: Check if this is necessary
    conn.register_observer_events()
        .await
        .map_err(|e| anyhow!("Got error while register events: {:?}", e))?;

    if monitor_channel.valid() {
        conn.register_channel_events()
            .await
            .map_err(|e| anyhow!("Register monitor channel error: {:?}", e))?;
    }

    let mut received = true;

    if !whitelist_ip.is_empty() {
        conn.write_data("banlist\n\r").await.ok();
    }

    loop {
        /*if recv
            .has_changed()
            .map_err(|e| anyhow!("Got error in check watcher {:?}", e))?
        {
            info!("Exit from staff thread!");
            conn.logout().await.ok();
            break;
        }*/

        if let Ok(Some(message)) =
            tokio::time::timeout(Duration::from_millis(interval), recv.recv()).await
        {
            match message {
                PrivateMessageRequest::Message(client_id, message) => {
                    conn.send_text_message_unchecked(client_id, &message)
                        .await
                        .map(|_| trace!("Send message to {}", client_id))
                        .map_err(|e| {
                            anyhow!("Got error while send message to {} {:?}", client_id, e)
                        })?;
                }
                PrivateMessageRequest::Terminate => {
                    info!("Exit from staff thread!");
                    conn.logout().await.ok();
                    break;
                }
            }
        }

        //trace!("Read data");
        let data = conn
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error while read data: {:?}", e))?;
        //trace!("Read data end");

        if !matches!(&data, Some(x) if !x.is_empty()) {
            let mut signal = notify_signal.lock().await;
            if *signal {
                if !received {
                    error!("Not received answer after period of time");
                    return Err(anyhow!("Server disconnected"));
                }
                received = false;
                conn.write_data("whoami\n\rbanlist\n\r")
                    .await
                    .map_err(|e| {
                        error!("Got error while write data in keep alive function: {:?}", e)
                    })
                    .ok();
                *signal = false;
            }
            continue;
        }
        let data = data.unwrap();
        let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        //trace!("message loop start");
        for line in data.lines().map(|line| line.trim()) {
            if line.is_empty() {
                continue;
            }
            trace!("{}", line);

            staff(
                line,
                &mut received,
                &ignore_list,
                &monitor_channel,
                &whitelist_ip,
                &mut client_map,
                &sender,
                &current_time,
                &mut conn,
                output_file.clone(),
                config.mute_porter(),
            )
            .await?;
        }
        //trace!("message loop end");
    }
    monitor_channel
        .send_terminate()
        .await
        .map_err(|e| error!("{:?}", e))
        .ok();
    sender
        .send(TelegramData::Terminate)
        .await
        .map_err(|_| error!("Got error while send terminate signal"))
        .ok();
    Ok(())
}
