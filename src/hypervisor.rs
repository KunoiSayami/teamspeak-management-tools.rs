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
    use log::{error, info, trace, warn};
    use std::sync::Arc;
    use std::time::Duration;
    use tap::TapFallible;
    #[cfg(feature = "tracker")]
    use tap::TapOptional;
    use tokio::sync::{mpsc, Barrier, Notify};

    async fn try_init_connection(
        config: &Config,
        sid: i64,
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
        notifier: Arc<Notify>,
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
                panic!("Main handler");
                //return Err(SubThreadExitReason::from())
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
        tokio::try_join!(
            auto_channel_handler,
            telegram_handler,
            #[cfg(feature = "tracker")]
            user_tracker.wait(),
            /*tokio::spawn(async {
                notifier.notified().await;
                error!("Force exit program (waiting sqlite handler).");
                return Err("Sqlite handler".into());
            })*/
        )
        .map_err(|e| anyhow!("try_join! failed: {:?}", e))?;

        Ok(())
    }

    pub(super) async fn bootstrap(
        config: Config,
        notifier: Arc<Notify>,
        barrier: Arc<Barrier>,
    ) -> ClientResult<()> {
        // Await all client ready
        barrier.wait().await;
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

    pub(super) type ClientResult<T> = Result<T, SubThreadExitReason>;

    pub(super) enum SubThreadExitReason {
        Error(anyhow::Error),
        Abort(String),
        JoinError(tokio::task::JoinError),
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

    impl From<tokio::task::JoinError> for SubThreadExitReason {
        fn from(value: tokio::task::JoinError) -> Self {
            Self::JoinError(value)
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
                SubThreadExitReason::JoinError(e) => {
                    write!(f, "JoinError: {:?}", e)
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
    use std::future::Future;
    use std::path::Path;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Arc;
    use tokio::sync::{Barrier, Notify};
    use tokio::task::JoinHandle;

    #[derive(Debug)]
    pub struct Controller {
        join_handler: JoinHandle<anyhow::Result<()>>,
    }

    impl Controller {
        fn new(future: Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>) -> Self {
            Self {
                join_handler: tokio::spawn(future),
            }
        }

        pub async fn wait(self) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
            self.join_handler.await
        }

        pub async fn bootstrap_controller<P: AsRef<Path> + Debug>(
            paths: Vec<P>,
            notify: Arc<Notify>,
        ) -> anyhow::Result<Vec<Controller>> {
            let configures = paths
                .into_iter()
                .map(|path| {
                    let thread_id = Rc::new(uuid::Uuid::new_v4().to_string());
                    let ret = (
                        thread_id.clone(),
                        Config::try_from(path.as_ref())
                            .map_err(|e| anyhow!("{:?}: {}", path, e))?,
                    );
                    info!("Load {:?} as {}", &path, thread_id);
                    Ok(ret)
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let barrier = Arc::new(Barrier::new(configures.len()));

            let mut v = Vec::new();

            for (thread_id, config) in configures {
                let notify = notify.clone();
                let thread_id = Rc::into_inner(thread_id).unwrap();
                let barrier = barrier.clone();
                v.push(Controller::new(Box::pin(async move {
                    if let Err(e) = bootstrap(config, notify, barrier).await {
                        error!("Got error in {}: {:?}", thread_id, e);
                    }
                    Ok(())
                })));
            }

            Ok(v)
        }
    }
}

pub static SYSTEMD_MODE: OnceCell<bool> = OnceCell::new();
const SYSTEMD_MODE_RETRIES_TIMES: u32 = 3;

pub use controller::Controller;
use once_cell::sync::OnceCell;
use types::ClientResult;
