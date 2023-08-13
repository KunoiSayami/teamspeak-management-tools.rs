mod auto_channel;
mod configure;
mod datastructures;
mod hypervisor;
mod observer;
mod plugins;
mod socketlib;

use crate::hypervisor::{Controller, SYSTEMD_MODE};
use clap::{arg, command};
use log::{error, LevelFilter};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::Notify;

const DEFAULT_OBSERVER_NICKNAME: &str = "observer";
const DEFAULT_AUTO_CHANNEL_NICKNAME: &str = "auto channel";

pub static OBSERVER_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();
pub static AUTO_CHANNEL_NICKNAME_OVERRIDE: OnceCell<String> = OnceCell::new();

async fn start_services(configs: Vec<String>, systemd_mode: bool) -> anyhow::Result<()> {
    let notify = Arc::new(Notify::new());

    let controllers = Controller::bootstrap_controller(configs, notify.clone()).await?;

    SYSTEMD_MODE.set(systemd_mode).unwrap();

    tokio::select! {
        _ = async {
            tokio::signal::ctrl_c().await.unwrap();
            // First ctrl_c signal
            notify.notify_waiters();
            tokio::signal::ctrl_c().await.unwrap();
            notify.notify_waiters();
            tokio::signal::ctrl_c().await.unwrap();
            error!("Force exit!");
            std::process::exit(137);
        } => {
            unreachable!()
        }
        _ = async move {
            for controller in controllers {
                controller.wait().await;
            }
        } => {

        }
    }

    Ok(())
}

fn build_logger(count: u8) {
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
    builder.init();
}

#[allow(unreachable_code)]
fn main() -> anyhow::Result<()> {
    let matches = command!()
        .args(&[
            arg!([CONFIG_FILE] ... "Override default configure file location")
                .default_value("config.toml"),
            arg!(--systemd "Start in systemd mode, which enable wait if connect failed"),
            arg!(--"observer-name" [OBSERVER_NAME] "Override observer nickname"),
            arg!(--"autochannel-name" [AUTO_CHANNEL_NAME] "Override auto channel nickname"),
            arg!(-d --debug ... "Enable debug mode (can specify more times)"),
        ])
        .get_matches();

    build_logger(*matches.get_one::<u8>("debug").unwrap_or(&0));

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

    let configure_paths = matches.get_many::<String>("CONFIG_FILE");
    let configure = configure_paths.unwrap().map(|v| v.to_string()).collect();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start_services(configure, matches.get_flag("systemd")))?;

    Ok(())
}
