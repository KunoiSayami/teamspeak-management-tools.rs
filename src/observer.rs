use crate::auto_channel::AutoChannelInstance;
use crate::configure::Config;
use crate::datastructures::EventHelperTrait;
use crate::datastructures::{
    BanEntry, FromQueryString, NotifyClientEnterView, NotifyClientLeftView, NotifyClientMovedView,
    NotifyTextMessage,
};
use crate::socketlib::SocketConn;
use crate::{DEFAULT_OBSERVER_NICKNAME, OBSERVER_NICKNAME_OVERRIDE};
use anyhow::anyhow;
use futures_util::future::FutureExt;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::fmt::Formatter;
use std::hint::unreachable_unchecked;
use std::sync::Arc;
use std::time::Duration;
use tap::{Tap, TapFallible, TapOptional};
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::mpsc;

pub enum PrivateMessageRequest {
    Message(i64, Arc<String>),
    KeepAlive,
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
    ignore_list: &[String],
    monitor_channel: &AutoChannelInstance,
    whitelist_ip: &Vec<String>,
    client_map: &mut HashMap<i64, (String, bool)>,
    sender: &mpsc::Sender<TelegramData>,
    current_time: &str,
    conn: &mut SocketConn,
    tracker_controller: &(dyn EventHelperTrait + Send + Sync),
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
                    .tap_err(|_| error!("Got error while send data to telegram"))
                    .ok()),
            async {
                #[cfg(feature = "tracker")]
                tracker_controller
                    .insert(
                        view.client_id() as i32,
                        Some(view.client_unique_identifier().to_string()),
                        Some(view.client_nickname().to_string()),
                        Some(view.channel_id() as i32),
                    )
                    .await
                    .tap_none(|| warn!("Unable send message to tracker"))
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
            .tap_err(|_| error!("Got error while send data to telegram"))
            .ok();
        tracker_controller
            .insert(
                view.client_id() as i32,
                None,
                Some(nickname.0.clone()),
                None,
            )
            .await
            .tap_none(|| warn!("Unable send message to tracker"));
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
        #[cfg(feature = "tracker")]
        tracker_controller
            .insert(
                view.client_id() as i32,
                Some(view.invoker_uid().to_string()),
                Some(view.invoker_name().to_string()),
                Some(view.channel_id() as i32),
            )
            .await
            .tap_none(|| warn!("Unable send message to tracker"));
        return Ok(());
    }

    if line.contains("notifytextmessage") && monitor_channel.valid() {
        let view = NotifyTextMessage::from_query(line)
            .map_err(|e| anyhow!("Got error while deserialize moved view: {:?}", e))?;

        if !view.msg().eq("!reset") {
            return Ok(());
        }
        monitor_channel
            .send_delete(view.invoker_id(), view.invoker_uid().to_string())
            .await
            .tap(|_| {
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
        return Ok(());
    }
    Ok(())
}

pub async fn observer_thread(
    mut conn: SocketConn,
    mut recv: mpsc::Receiver<PrivateMessageRequest>,
    telegram_sender: mpsc::Sender<TelegramData>,
    monitor_channel: AutoChannelInstance,
    config: Config,
    tracker_controller: Box<dyn EventHelperTrait + Send + Sync>,
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

    conn.change_nickname(
        OBSERVER_NICKNAME_OVERRIDE.get_or_init(|| DEFAULT_OBSERVER_NICKNAME.to_string()),
    )
    .await
    .map_err(|e| anyhow!("Got error while change nickname: {:?}", e))?;

    let mut client_map: HashMap<i64, (String, bool)> = HashMap::new();

    for client in conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("QueryClient failure: {:?}", e))?
    {
        if client_map.get(&client.client_id()).is_some() || !client.client_is_user() {
            continue;
        }

        client_map.insert(
            client.client_id(),
            (client.client_nickname().to_string(), false),
        );
        tracker_controller
            .insert(
                client.client_id() as i32,
                Some(format!("{}", client.client_database_id())),
                Some(client.client_nickname().to_string()),
                Some(client.channel_id() as i32),
            )
            .await
            .tap_none(|| warn!("Unable send insert request"));
    }

    // TODO: Check if this is necessary
    conn.register_observer_events()
        .await
        .map_err(|e| anyhow!("Got error while register events: {:?}", e))?;

    if monitor_channel.valid() {
        conn.register_channel_events()
            .await
            .map_err(|e| anyhow!("Register monitor channel error: {:?}", e))?;
    }

    if !whitelist_ip.is_empty() {
        conn.write_data("banlist\n\r").await.ok();
    }

    loop {
        tokio::select! {
            message = tokio::time::timeout(Duration::from_millis(interval), recv.recv()) => {
                let message = match message {
                    Ok(Some(ret)) => ret,
                    _ => continue,
                };
                match message {
                    PrivateMessageRequest::Message(client_id, message) => {
                        conn.send_text_message_unchecked(client_id, &message)
                        .await
                        .map(|_| trace!("Send message to {}", client_id))
                        .map_err(|e| {
                            anyhow!("Got error while send message to {} {:?}", client_id, e)
                        })?;
                        continue
                    }
                    PrivateMessageRequest::KeepAlive => {
                        conn.send_keepalive().await
                            .map_err(|e| {
                                anyhow!("Got error while write data in keep alive function: {:?}", e)
                            })?;
                    }
                    PrivateMessageRequest::Terminate => {
                        info!("Exit from staff thread!");
                        conn.logout().await.ok();
                        break;
                    }
                }
            }
            ret = conn.wait_readable() => {
                if !ret? {
                    continue
                }
            }
        }

        let data = conn
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error while read data: {:?}", e))?;

        if !matches!(&data, Some(x) if !x.is_empty()) {
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
                &ignore_list,
                &monitor_channel,
                &whitelist_ip,
                &mut client_map,
                &telegram_sender,
                &current_time,
                &mut conn,
                tracker_controller.as_ref(),
            )
            .await?;
        }
        //trace!("message loop end");
    }

    monitor_channel
        .send_terminate()
        .await
        .tap_err(|e| error!("{:?}", e))
        .ok();

    telegram_sender
        .send(TelegramData::Terminate)
        .await
        .tap_err(|_| error!("Got error while send terminate signal"))
        .ok();
    Ok(())
}
