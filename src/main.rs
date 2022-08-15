mod auto_channel;
mod datastructures;
mod observer;
mod socketlib;

use crate::auto_channel::{auto_channel_staff, AutoChannelInstance, MSG_MOVE_TO_CHANNEL};
use crate::datastructures::Config;
use crate::observer::{observer_thread, telegram_thread, PrivateMessageRequest};
use crate::socketlib::SocketConn;
use anyhow::anyhow;
use clap::{arg, Command};
use futures_util::TryFutureExt;
use log::{debug, error, info, trace, warn, LevelFilter};
use once_cell::sync::OnceCell;
use std::hint::unreachable_unchecked;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

static SYSTEMD_MODE: OnceCell<bool> = OnceCell::new();
const SYSTEMD_MODE_RETRIE_TIMES: u32 = 3;

async fn try_init_connection(
    config: &Config,
    sid: i64,
) -> anyhow::Result<(SocketConn, SocketConn)> {
    let retries = if *SYSTEMD_MODE.get().unwrap() {
        debug!("Systemd mode is present, will retry if connection failed.");
        SYSTEMD_MODE_RETRIE_TIMES
    } else {
        1
    };
    for step in 0..retries {
        match init_connection(config, sid).await {
            Ok(ret) => {
                return Ok((
                    ret,
                    init_connection(config, sid).await.map_err(|e| {
                        anyhow!("Got error while create second connection: {:?}", e)
                    })?,
                ))
            }
            Err(e) => {
                if retries == SYSTEMD_MODE_RETRIE_TIMES && step < retries - 1 {
                    warn!("Connect server error, will retry after 10 seconds, {}", e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
    unsafe { unreachable_unchecked() }
}

async fn init_connection(config: &Config, sid: i64) -> anyhow::Result<SocketConn> {
    let cfg = config.raw_query();
    let mut conn = SocketConn::connect(&cfg.server(), cfg.port()).await?;
    conn.login(cfg.user(), &cfg.password())
        .await
        .map_err(|e| anyhow!("Login failed. {:?}", e))?;

    conn.select_server(sid)
        .await
        .map_err(|e| anyhow!("Select server id failed: {:?}", e))?;

    Ok(conn)
}

async fn watchdog(conn: (SocketConn, SocketConn), config: Config) -> anyhow::Result<()> {
    let (conn1, conn2) = conn;

    //let (exit_sender, exit_receiver) = watch::channel(false);
    let (private_message_sender, private_message_receiver) = mpsc::channel(4096);
    let (trigger_sender, trigger_receiver) = mpsc::channel(1024);
    let (telegram_sender, telegram_receiver) = mpsc::channel(4096);
    let keepalive_signal = Arc::new(Mutex::new(false));
    let alt_signal = keepalive_signal.clone();

    let auto_channel_handler = tokio::spawn(auto_channel_staff(
        conn2,
        config.server().channels(),
        config.server().privilege_group_id(),
        config.server().redis_server(),
        config.misc().interval(),
        trigger_receiver,
        config.channel_permissions(),
        private_message_sender.clone(),
    ));

    let auto_channel_instance =
        AutoChannelInstance::new(config.server().channels(), Some(trigger_sender));

    let observer_handler = tokio::spawn(observer_thread(
        conn1,
        private_message_receiver,
        telegram_sender,
        config.misc().interval(),
        alt_signal,
        config.server().ignore_user_name(),
        auto_channel_instance,
        config.server().whitelist_ip(),
    ));

    let telegram_handler = tokio::spawn(telegram_thread(
        config.telegram().api_key().to_string(),
        config.telegram().target(),
        config.telegram().api_server(),
        telegram_receiver,
    ));

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT, send signal to thread.");
            private_message_sender // TODO: check performance
                .send(PrivateMessageRequest::Terminate)
                .map_err(|_| error!("Send terminate error"))
                .await
                .ok();
            trace!("Send signal!");
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program.");
            std::process::exit(137);
        } => {
        }
        _ = async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let mut i = keepalive_signal.lock().await;
                *i = true;
            }
        } => {}
        ret = observer_handler => {
            ret??
        }
    }

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program.");
            std::process::exit(137);
        } => {

        }
        ret = auto_channel_handler => {
            ret??;
        }
    }

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program.");
            std::process::exit(137);
        } => {

        }
        ret = telegram_handler => {
            ret??;
        }
    }

    Ok(())
}

async fn configure_file_bootstrap<P: AsRef<Path>>(
    path: P,
    systemd_mode: bool,
) -> anyhow::Result<()> {
    let config = Config::try_from(path.as_ref())?;
    MSG_MOVE_TO_CHANNEL
        .set(config.message().move_to_channel())
        .unwrap();
    SYSTEMD_MODE
        .set(config.misc().systemd() || systemd_mode)
        .unwrap();
    watchdog(
        try_init_connection(&config, config.server().server_id()).await?,
        config,
    )
    .await
}

fn main() -> anyhow::Result<()> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .args(&[
            arg!([CONFIG_FILE] "Override default configure file location"),
            arg!(--systemd "Start in systemd mode, which enable wait if connect failed"),
        ])
        .get_matches();

    env_logger::Builder::from_default_env()
        .filter_module("rustls", LevelFilter::Warn)
        .filter_module("reqwest", LevelFilter::Warn)
        .init();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(configure_file_bootstrap(
            matches.value_of("CONFIG_FILE").unwrap_or("config.toml"),
            matches.is_present("systemd"),
        ))?;

    Ok(())
}
