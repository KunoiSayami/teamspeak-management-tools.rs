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
use log::{LevelFilter, error, info};
use std::io::Write as _;
use std::sync::{Arc, OnceLock};
use tokio::sync::Notify;

const DEFAULT_OBSERVER_NICKNAME: &str = "observer";
const DEFAULT_AUTO_CHANNEL_NICKNAME: &str = "auto channel";
const DEFAULT_LEVEL_DB_LOCATION: &str = "./level.db";

pub static OBSERVER_NICKNAME_OVERRIDE: OnceLock<String> = OnceLock::new();
pub static AUTO_CHANNEL_NICKNAME_OVERRIDE: OnceLock<String> = OnceLock::new();

/// Waits for the next shutdown signal: SIGINT, or SIGTERM on Unix.
///
/// Both handles are created once and reused. tokio's `ctrl_c()` is an async
/// fn rather than a handle, and on Unix tokio installs the OS handler on
/// first registration and never resets it, so re-registering per call is
/// both pointless and a failure mode.
#[cfg(unix)]
async fn wait_for_shutdown_signal(
    sigint: &mut tokio::signal::unix::Signal,
    sigterm: &mut tokio::signal::unix::Signal,
) {
    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal(_sigint: &mut (), _sigterm: &mut ()) {
    tokio::signal::ctrl_c().await.ok();
}

async fn start_services(config: String, systemd_mode: bool) -> anyhow::Result<()> {
    let notify = Arc::new(Notify::new());
    let exit_notify = Arc::new(Notify::new());

    #[cfg(unix)]
    let (mut sigint, mut sigterm) = {
        use tokio::signal::unix::{SignalKind, signal};
        // If SIGTERM cannot be installed, still honour SIGINT rather than
        // failing startup: losing graceful shutdown must not stop the daemon.
        let sigterm = signal(SignalKind::terminate())
            .or_else(|_| signal(SignalKind::interrupt()))
            .expect("shutdown signal handler must install");
        let sigint = signal(SignalKind::interrupt()).expect("SIGINT handler must install");
        (sigint, sigterm)
    };
    #[cfg(not(unix))]
    let (mut sigint, mut sigterm) = ((), ());

    SYSTEMD_MODE.set(systemd_mode).unwrap();

    let (kv_backend, controllers, telegram_handler) =
        Controller::bootstrap_controller(config, notify.clone(), exit_notify.clone()).await?;

    tokio::select! {
        _ = async {
            // First signal: ask every controller to shut down gracefully.
            wait_for_shutdown_signal(&mut sigint, &mut sigterm).await;
            notify.notify_waiters();
            // Second: tell them to stop waiting for their children.
            wait_for_shutdown_signal(&mut sigint, &mut sigterm).await;
            notify.notify_waiters();
            // Third: give up on graceful shutdown.
            wait_for_shutdown_signal(&mut sigint, &mut sigterm).await;
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
                if tokio::time::timeout(tokio::time::Duration::from_secs(10), exit_notify.notified()).await.is_ok(){
                    tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                    break
                }
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
            info!("Exited thread number: {}", ret.into_iter().collect::<Result<Vec<_>, _>>()?.len());
        }
    }

    kv_backend.disconnect().await?;

    telegram_handler.await??;
    Ok(())
}

/// Maps the number of `-d` flags to a base log level.
///
/// `from_default_env` defaults to `error` when RUST_LOG is unset, so without
/// this `-d` had no effect at all.
fn level_for_debug_count(count: u8) -> LevelFilter {
    match count {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
}

fn build_logger(count: u8, systemd_mode: bool) {
    let mut builder = env_logger::Builder::from_default_env();
    // An empty RUST_LOG counts as unset: `RUST_LOG= cmd` would otherwise keep
    // env_logger's `error` default and make -d look broken.
    let rust_log_unset = std::env::var("RUST_LOG")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true);
    if rust_log_unset {
        builder.filter_level(level_for_debug_count(count));
    }
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
            arg!(--"observer-name" <OBSERVER_NAME> "Override observer nickname"),
            arg!(--"autochannel-name" <AUTO_CHANNEL_NAME> "Override auto channel nickname"),
            arg!(-d --debug ... "Enable debug mode (can specify more times)"),
        ])
        .get_matches();

    let systemd_mode = matches.get_flag("systemd");
    build_logger(matches.get_count("debug"), systemd_mode);

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

#[cfg(test)]
mod test {
    use super::level_for_debug_count;
    use log::LevelFilter;

    #[test]
    fn debug_count_maps_to_levels() {
        assert_eq!(level_for_debug_count(0), LevelFilter::Info);
        assert_eq!(level_for_debug_count(1), LevelFilter::Debug);
        assert_eq!(level_for_debug_count(2), LevelFilter::Trace);
        assert_eq!(level_for_debug_count(200), LevelFilter::Trace);
    }
}
