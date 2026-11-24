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

/// Inbound-activity watchdog for the ServerQuery connection.
///
/// A half-open TCP connection accepts writes forever but delivers nothing.
/// The hypervisor sends a keepalive every 30 seconds and a live server always
/// answers it, so a healthy link produces inbound data at least that often.
/// Silence beyond `STALL_AFTER` therefore means the link is dead even while
/// writes still succeed.
struct Heartbeat {
    last_rx: std::time::Instant,
}

impl Heartbeat {
    /// Three keepalive periods: two missed replies plus one period of slack.
    const STALL_AFTER: Duration = Duration::from_secs(90);

    fn new() -> Self {
        Self {
            last_rx: std::time::Instant::now(),
        }
    }

    /// Called whenever any bytes are received.
    fn note_rx(&mut self) {
        self.last_rx = std::time::Instant::now();
    }

    fn is_stalled(&self) -> bool {
        self.last_rx.elapsed() >= Self::STALL_AFTER
    }
}

/// Re-establishes everything that is per-connection.
///
/// Must run on every fresh connection: nickname, tracker backfill,
/// notification registrations and the whitelist ban sweep. TeamSpeak
/// registrations do not survive a reconnect, so skipping any of these
/// silently disables a feature.
async fn prepare_connection(
    conn: &mut SocketConn,
    monitor_channel: &AutoChannelInstance,
    tracker_controller: &(dyn EventHelperTrait + Send + Sync),
    whitelist_ip: &[String],
    client_map: &mut HashMap<i64, (String, bool)>,
    thread_id: &str,
) -> anyhow::Result<()> {
    conn.change_nickname(
        OBSERVER_NICKNAME_OVERRIDE.get_or_init(|| DEFAULT_OBSERVER_NICKNAME.to_string()),
    )
    .await
    .map_err(|e| anyhow!("Got error while change nickname: {e:?}"))?;

    // Client ids are reassigned on a new connection; stale entries would
    // mislabel later leave notifications.
    client_map.clear();

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

    conn.register_observer_events()
        .await
        .map_err(|e| anyhow!("Got error while register events: {e:?}"))?;

    if monitor_channel.valid() {
        conn.register_channel_events()
            .await
            .map_err(|e| anyhow!("Register monitor channel error: {e:?}"))?;
    }

    if !whitelist_ip.is_empty() {
        // The reply is consumed by the event loop and dispatched to
        // `Processor::ban_list`; do not read it here or whitelist cleanup
        // stops working.
        conn.write_data("banlist\n\r").await.ok();
    }

    Ok(())
}

enum ServeExit {
    Terminate,
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    conn: &mut SocketConn,
    recv: &mut mpsc::Receiver<PrivateMessageRequest>,
    telegram_sender: &BindTelegramHelper,
    monitor_channel: &AutoChannelInstance,
    tracker_controller: &(dyn EventHelperTrait + Send + Sync),
    ignore_list: &[String],
    whitelist_ip: &[String],
    thread_id: &str,
    interval: u64,
    client_map: &mut HashMap<i64, (String, bool)>,
) -> anyhow::Result<ServeExit> {
    let mut heartbeat = Heartbeat::new();

    loop {
        let data = tokio::select! {
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
                        // Half-open sockets accept writes forever, so a write
                        // cannot prove the link is alive. The keepalive reply is
                        // the only inbound traffic a quiet server produces.
                        if heartbeat.is_stalled() {
                            return Err(anyhow!(
                                "Keepalive stalled: no inbound data for {:?}",
                                Heartbeat::STALL_AFTER
                            ));
                        }
                        conn.send_keepalive().await
                            .map_err(|e| {
                                anyhow!("Got error while write data in keep alive function: {e:?}")
                            })?;
                        continue
                    }
                    PrivateMessageRequest::Terminate => {
                        info!("[{thread_id}] Exit from staff thread!");
                        conn.logout().await.ok();
                        return Ok(ServeExit::Terminate);
                    }
                }
            }
            data = conn.read_event() => {
                let data = data.map_err(|e| anyhow!("Got error while read data: {e:?}"))?;
                match data {
                    Some(s) if !s.is_empty() => {
                        heartbeat.note_rx();
                        s
                    }
                    // EOF means the peer closed the connection. Continuing
                    // here would spin forever on a dead socket.
                    _ => return Err(anyhow!("ServerQuery connection closed by peer")),
                }
            }
        };
        let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let arguments = Arguments::new(
            ignore_list,
            monitor_channel,
            whitelist_ip,
            telegram_sender,
            &current_time,
            tracker_controller,
            thread_id,
        );
        for line in data.lines().map(|line| line.trim()) {
            if line.is_empty() {
                continue;
            }
            trace!("[{thread_id}] {line}",);

            // One unhandleable event must not end the whole observer: a single
            // malformed or unknown notification used to propagate out of here
            // and take the process down with it. Socket-level read failures
            // still terminate the loop, since those are handled above.
            if let Err(e) = staff(line, client_map, conn, &arguments).await {
                error!("[{thread_id}] Unable handle event {line:?}: {e:?}");
            }
        }
    }
}

/// Waits `dur`, returning false if a Terminate request (or channel close)
/// arrives first, so shutdown is never delayed by a backoff sleep.
async fn sleep_or_terminate(
    recv: &mut mpsc::Receiver<PrivateMessageRequest>,
    dur: Duration,
) -> bool {
    !matches!(
        tokio::time::timeout(dur, recv.recv()).await,
        Ok(Some(PrivateMessageRequest::Terminate)) | Ok(None)
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn observer_thread(
    raw_query: crate::configure::config::RawQuery,
    server_id: i64,
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

    let mut client_map: HashMap<i64, (String, bool)> = HashMap::new();
    let mut backoff = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(60);
    // A genuinely broken config (bad credentials, missing permissions) will
    // never succeed. Give up after this many consecutive failures so the
    // process exits loudly instead of retrying in silence forever.
    const MAX_SETUP_FAILURES: u32 = 10;
    let mut setup_failures = 0u32;

    loop {
        let mut conn = match crate::hypervisor::connect_raw(&raw_query, server_id).await {
            Ok(conn) => conn,
            Err(e) => {
                setup_failures += 1;
                error!(
                    "[{thread_id}] Reconnect failed ({setup_failures}/{MAX_SETUP_FAILURES}): {e:?}"
                );
                if setup_failures >= MAX_SETUP_FAILURES {
                    return Err(anyhow!(
                        "Unable to reconnect after {MAX_SETUP_FAILURES} attempts: {e:?}"
                    ));
                }
                if !sleep_or_terminate(&mut recv, backoff).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        if let Err(e) = prepare_connection(
            &mut conn,
            &monitor_channel,
            tracker_controller.as_ref(),
            &whitelist_ip,
            &mut client_map,
            &thread_id,
        )
        .await
        {
            setup_failures += 1;
            error!("[{thread_id}] Setup failed ({setup_failures}/{MAX_SETUP_FAILURES}): {e:?}");
            if setup_failures >= MAX_SETUP_FAILURES {
                return Err(anyhow!(
                    "Unable to set up the connection after {MAX_SETUP_FAILURES} attempts: {e:?}"
                ));
            }
            if !sleep_or_terminate(&mut recv, backoff).await {
                return Ok(());
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
            continue;
        }

        setup_failures = 0;
        backoff = Duration::from_secs(1);
        info!("[{thread_id}] Observer connected");

        match serve(
            &mut conn,
            &mut recv,
            &telegram_sender,
            &monitor_channel,
            tracker_controller.as_ref(),
            &ignore_list,
            &whitelist_ip,
            &thread_id,
            interval,
            &mut client_map,
        )
        .await
        {
            Ok(ServeExit::Terminate) => break,
            Err(e) => {
                // Always back off on disconnect too. Without this, a peer that
                // accepts and immediately closes would spin the loop at full
                // speed.
                error!("[{thread_id}] Connection lost: {e:?}; reconnecting in {backoff:?}");
                if !sleep_or_terminate(&mut recv, backoff).await {
                    break;
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }

    monitor_channel
        .send_terminate()
        .await
        .inspect_err(|e| error!("[{thread_id}] {e:?}"))
        .ok();

    Ok(())
}

#[cfg(test)]
mod test {
    use super::Heartbeat;
    use std::time::{Duration, Instant};

    #[test]
    fn heartbeat_detects_silence_but_not_activity() {
        let mut hb = Heartbeat::new();
        assert!(!hb.is_stalled());
        hb.note_rx();
        assert!(!hb.is_stalled());
    }

    #[test]
    fn heartbeat_stalls_after_the_threshold() {
        let mut hb = Heartbeat::new();
        hb.last_rx = Instant::now() - Heartbeat::STALL_AFTER - Duration::from_secs(1);
        assert!(hb.is_stalled());
    }
}
