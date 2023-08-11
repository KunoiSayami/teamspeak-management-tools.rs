mod inner {
    use super::ClientResult;
    use super::SYSTEMD_MODE;
    use super::SYSTEMD_MODE_RETRIES_TIMES;
    use crate::auto_channel::{auto_channel_staff, AutoChannelInstance};
    use crate::configure::config::RawQuery;
    use crate::configure::Config;
    #[cfg(feature = "tracker")]
    use crate::datastructures::EventHelperTrait;
    #[cfg(not(feature = "tracker"))]
    use crate::datastructures::PseudoEventHelper;
    use crate::observer::{observer_thread, telegram_thread, PrivateMessageRequest};
    #[cfg(feature = "tracker")]
    use crate::plugins::tracker::DatabaseHelper;
    use crate::socketlib::SocketConn;
    use anyhow::anyhow;
    use futures_util::TryFutureExt;
    use log::{debug, error, info, trace, warn};
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Duration;
    use tap::TapFallible;
    #[cfg(feature = "tracker")]
    use tap::TapOptional;
    use tokio::sync::{mpsc, Notify};

    async fn try_init_connection(
        config: &Config,
        sid: i64,
        thread_id: Rc<String>,
    ) -> anyhow::Result<(SocketConn, SocketConn)> {
        let retries = if *SYSTEMD_MODE.get().unwrap() {
            //debug!("Systemd mode is present, will retry if connection failed.");
            SYSTEMD_MODE_RETRIES_TIMES
        } else {
            1
        };
        for step in 0..retries {
            match init_connection(config.raw_query(), sid).await {
                Ok(ret) => {
                    return Ok((
                        ret,
                        init_connection(config.raw_query(), sid)
                            .await
                            .map_err(|e| {
                                anyhow!("Got error while create second connection: {:?}", e)
                            })?,
                    ))
                }
                Err(e) => {
                    if retries == SYSTEMD_MODE_RETRIES_TIMES && step < retries - 1 {
                        warn!("Connect server error, will retry after 10 seconds, {}", e);
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    }

    async fn init_connection(cfg: &RawQuery, sid: i64) -> anyhow::Result<SocketConn> {
        let mut conn = SocketConn::connect(&cfg.server(), cfg.port()).await?;
        conn.login(cfg.user(), cfg.password())
            .await
            .map_err(|e| anyhow!("Login failed. {:?}", e))?;

        conn.select_server(sid)
            .await
            .map_err(|e| anyhow!("Select server id failed: {:?}", e))?;

        Ok(conn)
    }

    async fn watchdog(
        conn: (SocketConn, SocketConn),
        config: Config,
        notifier: Notify,
    ) -> ClientResult<()> {
        let (observer_connection, auto_channel_connection) = conn;

        let (private_message_sender, private_message_receiver) = mpsc::channel(4096);
        let (trigger_sender, trigger_receiver) = mpsc::channel(1024);
        let (telegram_sender, telegram_receiver) = mpsc::channel(4096);

        #[cfg(feature = "tracker")]
        let (user_tracker, tracker_controller) =
            DatabaseHelper::safe_new(config.server().track_channel_member().clone(), |e| {
                error!("Unable to create tracker {:?}", e)
            })
            .await;

        #[cfg(not(feature = "tracker"))]
        let tracker_controller = PseudoEventHelper::new();

        let auto_channel_handler = tokio::spawn(auto_channel_staff(
            auto_channel_connection,
            trigger_receiver,
            private_message_sender.clone(),
            config.clone(),
        ));

        let auto_channel_instance =
            AutoChannelInstance::new(config.server().channels(), Some(trigger_sender));

        let observer_handler = tokio::spawn(observer_thread(
            observer_connection,
            private_message_receiver,
            telegram_sender,
            auto_channel_instance,
            config.clone(),
            Box::new(tracker_controller.clone()),
        ));

        let telegram_handler = tokio::spawn(telegram_thread(
            config.telegram().api_key().to_string(),
            config.telegram().target(),
            config.telegram().api_server(),
            telegram_receiver,
        ));

        tokio::select! {
            _ = async {
                notifier.notified().await;
                info!("Recv SIGINT, send signal to thread.");
                private_message_sender
                    .send(PrivateMessageRequest::Terminate)
                    .map_err(|_| error!("Send terminate error"))
                    .await
                    .ok();
                #[cfg(feature = "tracker")]
                tracker_controller
                    .terminate()
                    .await
                    .tap_none(|| error!("Send tracker terminate error"));
                trace!("Send signal!");
                notifier.notified().await;
                error!("Force exit program.");
                return Err("Main handler".into())
            } => {
                unreachable!()
            }
            _ = async {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    private_message_sender.send(PrivateMessageRequest::KeepAlive)
                        .await
                        .tap_err(|_| error!("Send keep alive command error"))
                        .ok();
                }
            } => {
                    unreachable!()
            }
            ret = observer_handler => {
                ret??
            }
        }

        // TODO: Need handle error
        let item = tokio::try_join!(
            auto_channel_handler,
            telegram_handler,
            user_tracker.wait(),
            tokio::spawn(async {
                notifier.notified().await;
                error!("Force exit program (waiting sqlite handler).");
                return Err("Sqlite handler".into());
            })
        )
        .map_err(|e| anyhow!("try_join! failed: {:?}", e))?;

        Ok(())
    }

    pub async fn bootstrap<P: AsRef<Path>>(path: P, notifier: Notify) -> ClientResult<()> {
        let config = Config::try_from(path.as_ref())?;
        watchdog(
            try_init_connection(&config, config.server().server_id()).await?,
            config,
            notifier,
        )
        .await
    }
}

mod types {
    use anyhow::anyhow;
    use std::fmt::{Debug, Formatter};

    pub type ClientResult<T> = Result<T, SubThreadExitReason>;

    #[derive(Copy, Clone, Debug)]
    pub(super) enum HyperVisorEvent {
        Terminate,
    }

    pub(super) enum SubThreadExitReason {
        Error(anyhow::Error),
        Abort(String),
    }

    impl SubThreadExitReason {}

    impl From<tokio::io::Error> for SubThreadExitReason {
        fn from(value: tokio::io::Error) -> Self {
            Self::from(anyhow!("Got tokio::io::Error: {:?}", value))
        }
    }

    impl From<anyhow::Error> for SubThreadExitReason {
        fn from(value: anyhow::Error) -> Self {
            Self::Error(value)
        }
    }

    impl From<String> for SubThreadExitReason {
        fn from(value: String) -> Self {
            Self::Abort(value)
        }
    }

    impl From<&'static str> for SubThreadExitReason {
        fn from(value: &'static str) -> Self {
            Self::Abort(value.to_string())
        }
    }

    impl Debug for SubThreadExitReason {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Error(e) => {
                    write!(f, "Error: {:?}", e)
                }
                Self::Abort(abort_msg) => {
                    write!(f, "Abort from: {}", abort_msg)
                }
            }
        }
    }
}

mod controller {
    use super::inner::bootstrap;
    use crate::configure::Config;
    use anyhow::anyhow;
    use log::{error, info};
    use std::fmt::Debug;
    use std::path::Path;
    use std::rc::Rc;
    use tokio::sync::Notify;

    pub async fn start<P: AsRef<Path> + Debug>(path: P, notify: Notify) -> anyhow::Result<()> {
        if let Err(e) = bootstrap(path, notify).await {
            error!("Got error in {}: {:?}", thread_id, e);
        }
        Ok(())
    }

    pub async fn bootstrap_controller<P: AsRef<Path> + Debug>(
        paths: Vec<P>,
        systemd_mode: bool,
    ) -> anyhow::Result<()> {
        let notify = Notify::new();
        let configures = paths
            .into_iter()
            .map(|path| async {
                let thread_id = Rc::new(uuid::Uuid::new_v4().to_string());
                let ret = (
                    thread_id,
                    Config::try_from(path).map_err(|e| anyhow!("{:?}: {}", path, e))?,
                );
                info!("Load {:?} as {}", &path, thread_id);
                Ok(ret)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
    }
}

static SYSTEMD_MODE: OnceCell<bool> = OnceCell::new();
const SYSTEMD_MODE_RETRIES_TIMES: u32 = 3;

use once_cell::sync::OnceCell;
use types::ClientResult;
