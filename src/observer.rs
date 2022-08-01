use crate::datastructures::{
    FromQueryString, NotifyClientEnterView, NotifyClientLeftView, NotifyClientMovedView,
};

use crate::auto_channel::AutoChannelInstance;
use crate::socketlib::SocketConn;
use anyhow::anyhow;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::fmt::Formatter;
use std::hint::unreachable_unchecked;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::{mpsc, watch, Mutex};

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

pub async fn observer_thread(
    mut conn: SocketConn,
    mut recv: watch::Receiver<bool>,
    sender: mpsc::Sender<TelegramData>,
    interval: u64,
    notify_signal: Arc<Mutex<bool>>,
    ignore_list: Vec<String>,
    monitor_channel: AutoChannelInstance,
) -> anyhow::Result<()> {
    let mut client_map: HashMap<i64, (String, bool)> = HashMap::new();
    for client in conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("QueryClient failure: {:?}", e))?
    {
        if client_map.get(&client.client_id()).is_some() || client.client_type() == 1 {
            continue;
        }

        client_map.insert(
            client.client_id(),
            (client.client_nickname().to_string(), false),
        );
    }

    conn.register_observer_events()
        .await
        .map_err(|e| anyhow!("Got error while register events: {:?}", e))?;

    for channel_id in monitor_channel.channel_ids() {
        conn.register_auto_channel_events(*channel_id)
            .await
            .map_err(|e| anyhow!("Register monitor channel error: {:?}", e))?
    }

    let mut received = true;
    debug!("Loop running!");

    loop {
        if recv
            .has_changed()
            .map_err(|e| anyhow!("Got error in check watcher {:?}", e))?
        {
            info!("Exit from staff thread!");
            conn.logout().await.ok();
            break;
        }
        let data = conn
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error while read data: {:?}", e))?;

        if !matches!(&data, Some(x) if !x.is_empty()) {
            let mut signal = notify_signal.lock().await;
            if *signal {
                if !received {
                    error!("Not received answer after period of time");
                    return Err(anyhow!("Server disconnected"));
                }
                received = false;
                conn.write_data("whoami\n\r")
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
        for line in data.lines().map(|line| line.trim()) {
            if line.is_empty() {
                continue;
            }
            trace!("{}", line);
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
                    continue;
                }
                sender
                    .send(TelegramData::from_enter(current_time.clone(), view))
                    .await
                    .map_err(|_| error!("Got error while send data to telegram"))
                    .ok();
                continue;
            }
            if line.starts_with("notifyclientleftview") {
                let view = NotifyClientLeftView::from_query(line)
                    .map_err(|e| anyhow!("Got error while deserialize left view: {:?}", e))?;
                if !client_map.contains_key(&view.client_id()) {
                    warn!("Can't find client: {:?}", view.client_id());
                    continue;
                }
                let nickname = client_map.get(&view.client_id()).unwrap();
                if nickname.1 {
                    continue;
                }
                sender
                    .send(TelegramData::from_left(
                        current_time.clone(),
                        &view,
                        nickname.0.clone(),
                    ))
                    .await
                    .map_err(|_| error!("Got error while send data to telegram"))
                    .ok();
                client_map.remove(&view.client_id());
                continue;
            }
            if line.contains("notifyclientmoved") && monitor_channel.valid() {
                let view = NotifyClientMovedView::from_query(line)
                    .map_err(|e| anyhow!("Got error while deserialize moved view: {:?}", e))?;
                monitor_channel
                    .send(view)
                    .await
                    .map(|_| debug!("Notify auto channel thread"))?;
                continue;
            }
            if line.contains("virtualserver_status=") {
                received = true;
                continue;
            }
        }
        if let Ok(_) = tokio::time::timeout(Duration::from_millis(interval), recv.changed()).await {
            info!("Exit from staff thread!");
            conn.logout().await.ok();
            break;
        }
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
