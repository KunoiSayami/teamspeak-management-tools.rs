mod auto_channel;
mod datastructures;
mod observer;
mod socketlib;

use crate::auto_channel::{auto_channel_staff, AutoChannelInstance, MSG_MOVE_TO_CHANNEL};
use crate::datastructures::Config;
use crate::observer::{observer_thread, telegram_thread, PrivateMessageRequest};
use crate::socketlib::SocketConn;
use anyhow::anyhow;
use clap::{arg, command};
use futures_util::TryFutureExt;
use log::{debug, error, info, trace, warn, LevelFilter};
use once_cell::sync::OnceCell;
use std::hint::unreachable_unchecked;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

static AUTO_CHANNEL_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();
static OBSERVER_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();

const DEFAULT_OBSERVER_NICKNAME: &str = "observer";
const DEFAULT_AUTO_CHANNEL_NICKNAME: &str = "auto channel";

static SYSTEMD_MODE: OnceCell<bool> = OnceCell::new();
const SYSTEMD_MODE_RETRIES_TIMES: u32 = 3;

async fn try_init_connection(
    config: &Config,
    sid: i64,
) -> anyhow::Result<(SocketConn, SocketConn)> {
    let retries = if *SYSTEMD_MODE.get().unwrap() {
        debug!("Systemd mode is present, will retry if connection failed.");
        SYSTEMD_MODE_RETRIES_TIMES
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
                if retries == SYSTEMD_MODE_RETRIES_TIMES && step < retries - 1 {
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
    output_server_broadcast: Option<String>,
) -> anyhow::Result<()> {
    let (conn1, conn2) = conn;

    //let (exit_sender, exit_receiver) = watch::channel(false);
    let (private_message_sender, private_message_receiver) = mpsc::channel(4096);
    let (trigger_sender, trigger_receiver) = mpsc::channel(1024);
    let (telegram_sender, telegram_receiver) = mpsc::channel(4096);

    let auto_channel_handler = tokio::spawn(auto_channel_staff(
        conn2,
        trigger_receiver,
        private_message_sender.clone(),
        config.clone(),
    ));

    let auto_channel_instance =
        AutoChannelInstance::new(config.server().channels(), Some(trigger_sender));

    let observer_handler = tokio::spawn(observer_thread(
        conn1,
        private_message_receiver,
        telegram_sender,
        auto_channel_instance,
        config.clone(),
        output_server_broadcast,
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
            private_message_sender
                .send(PrivateMessageRequest::Terminate)
                .map_err(|_| error!("Send terminate error"))
                .await
                .ok();
            trace!("Send signal!");
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program.");
            std::process::exit(137);
        } => {
            unsafe { unreachable_unchecked() }
        }
        _ = async {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                private_message_sender.send(PrivateMessageRequest::KeepAlive)
                    .await
                    .map_err(|_| error!("Send keep alive command error"))
                    .ok();
            }
        } => {
            unsafe { unreachable_unchecked() }
        }
        ret = observer_handler => {
            ret??
        }
    }

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program (waiting auto channel handler).");
            std::process::exit(137);
        } => {
            unsafe { unreachable_unchecked() }
        }
        ret = auto_channel_handler => {
            ret??;
        }
    }

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit program (waiting telegram handler).");
            std::process::exit(137);
        } => {
            unsafe { unreachable_unchecked() }
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
    output_server_broadcast: Option<String>,
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
        output_server_broadcast,
    )
    .await
}

fn main() -> anyhow::Result<()> {
    let matches = command!()
        .args(&[
            arg!([CONFIG_FILE] "Override default configure file location"),
            arg!(--systemd "Start in systemd mode, which enable wait if connect failed"),
            arg!([SERVER_BROADCAST_OUTPUT_FILE] "Enable output server broadcast to file (beta)"),
            arg!(--"observer-name" [OBSERVER_NAME] "Override observer nickname"),
            arg!(--"autochannel-name" [AUTO_CHANNEL_NAME] "Override auto channel nickname"),
        ])
        .get_matches();

    env_logger::Builder::from_default_env()
        .filter_module("rustls", LevelFilter::Warn)
        .filter_module("reqwest", LevelFilter::Warn)
        .init();

    if let Some(nickname) = matches.get_one::<String>("observer-name") {
        OBSERVER_NICKNAME_OVERRIDE
            .set(nickname.to_string())
            .unwrap();
    }

    if let Some(nickname) = matches.get_one::<String>("autochannel-name") {
        AUTO_CHANNEL_NICKNAME_OVERRIDE
            .set(nickname.to_string())
            .unwrap();
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(configure_file_bootstrap(
            matches
                .get_one("CONFIG_FILE")
                .cloned()
                .unwrap_or_else(|| "config.toml".to_string()),
            matches.get_flag("systemd"),
            matches
                .get_one("SERVER_BROADCAST_OUTPUT_FILE")
                .map(|s: &String| s.to_string()),
        ))?;

    Ok(())
}
