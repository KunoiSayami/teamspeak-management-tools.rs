use crate::auto_channel::AutoChannelInstance;
use crate::configure::Config;
use crate::socketlib::SocketConn;
use crate::types::EventHelperTrait;
use crate::{DEFAULT_OBSERVER_NICKNAME, OBSERVER_NICKNAME_OVERRIDE};
use anyhow::anyhow;
use log::{error, info, trace, warn};
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Duration;
use tap::TapOptional;
use tokio::sync::mpsc;

pub enum PrivateMessageRequest {
    // Credit: SpriteOvO
    Message(i64, Cow<'static, str>),
    KeepAlive,
    Terminate,
}

struct Arguments<'a> {
    ignore_list: &'a [String],
    monitor_channel: &'a AutoChannelInstance,
    whitelist_ip: &'a [String],
    telegram_sender: &'a BindTelegramHelper,
    current_time: &'a str,
    tracker_controller: &'a (dyn EventHelperTrait + Send + Sync),
    thread_id: &'a str,
}

impl<'a> Arguments<'a> {
    pub fn ignore_list(&self) -> &'a [String] {
        self.ignore_list
    }
    pub fn monitor_channel(&self) -> &'a AutoChannelInstance {
        self.monitor_channel
    }
    pub fn whitelist_ip(&self) -> &'a [String] {
        self.whitelist_ip
    }
    pub fn telegram_sender(&self) -> &'a BindTelegramHelper {
        self.telegram_sender
    }
    pub fn current_time(&self) -> &'a str {
        self.current_time
    }

    pub fn tracker_controller(&self) -> &'a (dyn EventHelperTrait + Send + Sync) {
        self.tracker_controller
    }
    pub fn thread_id(&self) -> &'a str {
        self.thread_id
    }

    #[must_use]
    pub fn new(
        ignore_list: &'a [String],
        monitor_channel: &'a AutoChannelInstance,
        whitelist_ip: &'a [String],
        telegram_sender: &'a BindTelegramHelper,
        current_time: &'a str,
        tracker_controller: &'a (dyn EventHelperTrait + Send + Sync),
        thread_id: &'a str,
    ) -> Self {
        Self {
            ignore_list,
            monitor_channel,
            whitelist_ip,
            telegram_sender,
            current_time,
            tracker_controller,
            thread_id,
        }
    }
}

mod processor {
    use super::Arguments;
    use crate::socketlib::SocketConn;
    use crate::types::{
        BanEntry, FromQueryString, NotifyClientEnterView, NotifyClientLeftView,
        NotifyClientMovedView, NotifyTextMessage,
    };
    use anyhow::anyhow;
    use futures_util::FutureExt;
    use log::{error, info, trace, warn};
    use std::collections::HashMap;
    use tap::{Tap, TapOptional};

    type Result = anyhow::Result<()>;
    pub(super) struct Processor;

    impl Processor {
        pub(super) async fn user_enter(
            line: &str,
            argument: &Arguments<'_>,
            client_map: &mut HashMap<i64, (String, bool)>,
        ) -> Result {
            let view = NotifyClientEnterView::from_query(line)
                .map_err(|e| anyhow!("Got error while deserialize enter view: {e:?}"))?;
            let is_server_query = view.client_unique_identifier().eq("ServerQuery")
                || argument
                    .ignore_list()
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
                argument
                    .monitor_channel()
                    .send(view.clone().into())
                    .map(|result| {
                        result.map(|sent| {
                            if sent {
                                trace!("[{}] Notify auto channel thread", argument.thread_id())
                            }
                        })
                    }),
                argument
                    .telegram_sender()
                    .send_enter(argument.current_time().to_string(), &view)
                    .map(|result| result.tap_none(|| error!(
                        "[{}] Got error while send data to telegram",
                        argument.thread_id()
                    ))),
                async {
                    #[cfg(feature = "tracker")]
                    argument
                        .tracker_controller()
                        .insert(
                            view.client_id() as i32,
                            Some(view.client_unique_identifier().to_string()),
                            Some(view.client_nickname().to_string()),
                            Some(view.channel_id() as i32),
                        )
                        .await
                        .tap_none(|| {
                            warn!("[{}] Unable send message to tracker", argument.thread_id())
                        })
                }
            )
            .0?;

            Ok(())
        }

        pub(super) async fn user_left(
            line: &str,
            argument: &Arguments<'_>,
            client_map: &mut HashMap<i64, (String, bool)>,
        ) -> Result {
            let view = NotifyClientLeftView::from_query(line)
                .map_err(|e| anyhow!("Got error while deserialize left view: {e:?}"))?;
            if !client_map.contains_key(&view.client_id()) {
                warn!(
                    "[{}] Can't find client: {:?}",
                    argument.thread_id(),
                    view.client_id()
                );
                return Ok(());
            }
            let nickname = client_map.get(&view.client_id()).unwrap();
            if nickname.1 {
                return Ok(());
            }
            argument
                .telegram_sender()
                .send_left(
                    argument.current_time().to_string(),
                    &view,
                    nickname.0.clone(),
                )
                .await
                .tap_none(|| {
                    error!(
                        "[{}] Got error while send data to telegram",
                        argument.thread_id()
                    )
                });
            argument
                .tracker_controller()
                .insert(
                    view.client_id() as i32,
                    None,
                    Some(nickname.0.clone()),
                    None,
                )
                .await
                .tap_none(|| warn!("[{}] Unable send message to tracker", argument.thread_id()));
            client_map.remove(&view.client_id());
            Ok(())
        }

        pub(super) async fn user_move(line: &str, argument: &Arguments<'_>) -> Result {
            let view = NotifyClientMovedView::from_query(line)
                .map_err(|e| anyhow!("Got error while deserialize moved view: {e:?}"))?;
            argument
                .monitor_channel()
                .send(view.clone().into())
                .await
                .map(|sent| {
                    if sent {
                        trace!("[{}] Notify auto channel thread", argument.thread_id())
                    }
                })?;
            #[cfg(feature = "tracker")]
            argument
                .tracker_controller()
                .insert(
                    view.client_id() as i32,
                    Some(view.invoker_uid().to_string()),
                    Some(view.invoker_name().to_string()),
                    Some(view.channel_id() as i32),
                )
                .await
                .tap_none(|| warn!("[{}] Unable send message to tracker", argument.thread_id()));
            Ok(())
        }

        pub(super) async fn user_text(line: &str, argument: &Arguments<'_>) -> Result {
            let view = NotifyTextMessage::from_query(line)
                .map_err(|e| anyhow!("Got error while deserialize moved view: {e:?}"))?;

            if !view.msg().eq("!reset") {
                return Ok(());
            }
            argument
                .monitor_channel()
                .send_delete(view.invoker_id(), view.invoker_uid().to_string())
                .await
                .tap(|_| {
                    info!(
                        "[{}] Notify auto channel thread reset {}({})",
                        argument.thread_id(),
                        view.invoker_name(),
                        view.invoker_uid()
                    )
                })?;
            Ok(())
        }

        pub(super) async fn ban_list(
            line: &str,
            argument: &Arguments<'_>,
            conn: &mut SocketConn,
        ) -> Result {
            if argument.whitelist_ip().is_empty() {
                return Ok(());
            }
            for entry in line.split('|').map(BanEntry::from_query) {
                let entry = entry?;
                if argument.whitelist_ip().iter().any(|ip| entry.ip().eq(ip)) {
                    conn.ban_del(entry.ban_id()).await.map(|_| {
                        info!(
                            "[{}] Remove whitelist ip {} from ban list (was {entry})",
                            argument.thread_id(),
                            entry.ip(),
                        )
                    })?
                }
            }
            Ok(())
        }
    }
}
use crate::telegram::BindTelegramHelper;
use processor::Processor;

async fn staff(
    line: &str,
    client_map: &mut HashMap<i64, (String, bool)>,
    conn: &mut SocketConn,
    argument: &Arguments<'_>,
) -> anyhow::Result<()> {
    if line.starts_with("notifycliententerview") {
        return Processor::user_enter(line, argument, client_map).await;
    }

    if line.starts_with("notifyclientleftview") {
        return Processor::user_left(line, argument, client_map).await;
    }

    if line.contains("notifyclientmoved") && argument.monitor_channel().valid() {
        return Processor::user_move(line, argument).await;
    }

    if line.contains("notifytextmessage") && argument.monitor_channel().valid() {
        return Processor::user_text(line, argument).await;
    }
    if line.starts_with("banid") {
        return Processor::ban_list(line, argument, conn).await;
    }
    if line.contains("virtualserver_status=") {
        return Ok(());
    }
    Ok(())
}

pub async fn observer_thread(
    mut conn: SocketConn,
    mut recv: mpsc::Receiver<PrivateMessageRequest>,
    telegram_sender: BindTelegramHelper,
    monitor_channel: AutoChannelInstance,
    config: Config,
    tracker_controller: Box<dyn EventHelperTrait + Send + Sync>,
    thread_id: String,
) -> anyhow::Result<()> {
    let interval = config.misc().interval();
    let whitelist_ip = config.server().whitelist_ip();
    let ignore_list = config.server().ignore_user_name();
    info!(
        "[{thread_id}], interval: {interval}, ban list checker: {}, mute porter: {}",
        !whitelist_ip.is_empty(),
        config.mute_porter().enable()
    );

    conn.change_nickname(
        OBSERVER_NICKNAME_OVERRIDE.get_or_init(|| DEFAULT_OBSERVER_NICKNAME.to_string()),
    )
    .await
    .map_err(|e| anyhow!("Got error while change nickname: {e:?}"))?;

    let mut client_map: HashMap<i64, (String, bool)> = HashMap::new();

    for client in conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("QueryClient failure: {e:?}"))?
    {
        if client_map.contains_key(&client.client_id()) || !client.client_is_user() {
            continue;
        }

        client_map.insert(
            client.client_id(),
            (client.client_nickname().to_string(), false),
        );
        tracker_controller
            .insert(
                client.client_id() as i32,
                Some(client.client_database_id().to_string()),
                Some(client.client_nickname().to_string()),
                Some(client.channel_id() as i32),
            )
            .await
            .tap_none(|| warn!("[{thread_id}] Unable send insert request"));
    }

    // TODO: Check if this is necessary
    conn.register_observer_events()
        .await
        .map_err(|e| anyhow!("Got error while register events: {e:?}"))?;

    if monitor_channel.valid() {
        conn.register_channel_events()
            .await
            .map_err(|e| anyhow!("Register monitor channel error: {e:?}"))?;
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
                        .map(|_| trace!("[{thread_id}] Send message to {client_id}"))
                        .map_err(|e| {
                            anyhow!("[{thread_id}] Got error while send message to {client_id} {e:?}")
                        })?;
                        continue
                    }
                    PrivateMessageRequest::KeepAlive => {
                        conn.send_keepalive().await
                            .map_err(|e| {
                                anyhow!("Got error while write data in keep alive function: {e:?}")
                            })?;
                    }
                    PrivateMessageRequest::Terminate => {
                        info!("[{thread_id}] Exit from staff thread!");
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
            .map_err(|e| anyhow!("Got error while read data: {e:?}"))?;

        if !matches!(&data, Some(x) if !x.is_empty()) {
            continue;
        }
        let data = data.unwrap();
        let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let arguments = Arguments::new(
            &ignore_list,
            &monitor_channel,
            &whitelist_ip,
            &telegram_sender,
            &current_time,
            tracker_controller.as_ref(),
            &thread_id,
        );
        for line in data.lines().map(|line| line.trim()) {
            if line.is_empty() {
                continue;
            }
            trace!("[{thread_id}] {line}",);

            staff(line, &mut client_map, &mut conn, &arguments).await?;
        }
        //trace!("message loop end");
    }

    monitor_channel
        .send_terminate()
        .await
        .inspect_err(|e| error!("[{thread_id}] {e:?}"))
        .ok();

    Ok(())
}
