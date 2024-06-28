mod auto_channel;
mod configure;
mod hypervisor;
mod observer;
mod plugins;
mod socketlib;
mod telegram;
mod types;

use crate::hypervisor::{Controller, SYSTEMD_MODE};
use clap::{arg, command};
use log::{error, info, LevelFilter};
use once_cell::sync::OnceCell;
use std::io::Write as _;
use std::sync::Arc;
use tokio::sync::Notify;

const DEFAULT_OBSERVER_NICKNAME: &str = "observer";
const DEFAULT_AUTO_CHANNEL_NICKNAME: &str = "auto channel";
const DEFAULT_LEVEL_DB_LOCATION: &str = "./level.db";

pub static OBSERVER_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();
pub static AUTO_CHANNEL_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();

async fn start_services(config: String, systemd_mode: bool) -> anyhow::Result<()> {
    let notify = Arc::new(Notify::new());

    SYSTEMD_MODE.set(systemd_mode).unwrap();

    let (kv_backend, controllers, telegram_handler) =
        Controller::bootstrap_controller(config, notify.clone()).await?;

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            // First ctrl_c signal
            notify.notify_waiters();
            tokio::signal::ctrl_c().await.unwrap();
            // Notify again
            notify.notify_waiters();
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit!");
            std::process::exit(137);
        } => {
            unreachable!()
        }
        ret = async move {
            // Check if any thread has been exited
            let mut ret = Vec::new();
            loop {
                if controllers.iter().any(|x| x.is_finished()) {
                    break
                }
                tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
            }

            // Wait up to 300 micro secs
            tokio::time::sleep(tokio::time::Duration::from_micros(300)).await;

            for controller in controllers {
                if controller.is_finished() {
                    ret.push(controller.wait().await);
                }
            };
            ret
        } => {
            ret.into_iter().collect::<Result<Vec<_>, _>>()?;
        }
    }

    kv_backend.disconnect().await?;

    telegram_handler.await??;
    Ok(())
}

fn build_logger(count: u8, systemd_mode: bool) {
    let mut builder = env_logger::Builder::from_default_env();
    if count < 1 {
        builder.filter_module("sqlx", LevelFilter::Warn);
    }
    if count < 2 {
        builder
            .filter_module("h2", LevelFilter::Warn)
            .filter_module("hyper", LevelFilter::Warn);
    }
    if count < 3 {
        builder
            .filter_module("rustls", LevelFilter::Warn)
            .filter_module("reqwest", LevelFilter::Warn);
    }
    if systemd_mode {
        builder.format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()));
    }
    builder.init();
}

fn main() -> anyhow::Result<()> {
    let matches = command!()
        .args(&[
            arg!([CONFIG_FILE] "Override default configure file location")
                .default_value("config.toml"),
            arg!(--systemd "Start in systemd mode, which enable wait if connect failed"),
            arg!(--"observer-name" [OBSERVER_NAME] "Override observer nickname"),
            arg!(--"autochannel-name" [AUTO_CHANNEL_NAME] "Override auto channel nickname"),
            arg!(-d --debug ... "Enable debug mode (can specify more times)"),
        ])
        .get_matches();

    let systemd_mode = matches.get_flag("systemd");
    build_logger(*matches.get_one::<u8>("debug").unwrap_or(&0), systemd_mode);

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
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    let configure = matches.get_one::<String>("CONFIG_FILE").unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start_services(configure.clone(), systemd_mode))?;

    Ok(())
}
