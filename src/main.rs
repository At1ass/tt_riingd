mod controller;
mod interface;

use std::{fs::File, time::Duration};

use anyhow::{Result, anyhow};
use daemonize::Daemonize;
use event_listener::Listener;
use log::{LevelFilter, error, info};
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::time::interval;
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use unconfig::{config, configurable};
use zbus::connection;

use controller::{Controllers, DEFAULT_PERCENT};
use interface::DBusInterface;

fn init_log() -> Result<()> {
    syslog::unix(Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: "tt_riing_rs".into(),
        pid: 0,
    })
    .map_err(|e| anyhow!("{e}"))
    .and_then(|logger| {
        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|_| log::set_max_level(LevelFilter::Info))
            .map_err(|e| anyhow!("{e}"))
    })
}

fn into_daemon() -> Result<()> {
    File::create("/var/tmp/tt_riingd.log")
        .and_then(|out| Ok((out.try_clone()?, out)))
        .map_err(|e| anyhow!("{e}"))
        .and_then(|(stderr, stdout)| {
            Daemonize::new()
                .stdout(stdout)
                .stderr(stderr)
                .start()
                .map_err(|e| anyhow!("{e}"))
        })
}

#[configurable("${TT_RIINGD_CONFIG:config/config.yml}")]
struct Interface {
    version: String,
}

#[configurable("${TT_RIINGD_CONFIG:config/config.yml}")]
struct System {
    init_speed: u8,
    tick_seconds: u64,
}

#[tokio::main]
#[config(System, Interface)]
async fn tokio_main() -> Result<()> {
    let init_speed = CONFIG_SYSTEM.init_speed();
    let version = CONFIG_INTERFACE.version();
    let tick_seconds = CONFIG_SYSTEM.tick_seconds();

    let controllers = Controllers::init(init_speed)?;

    // First set
    controllers
        .send_init()
        .await
        .and(controllers.set_speed_for_all(DEFAULT_PERCENT).await)?;

    info!("Start — {init_speed}%");

    let _timer = tokio::spawn({
        let ctrls = controllers.clone();

        let mut interval_stream = IntervalStream::new(interval(Duration::from_secs(tick_seconds)));
        async move {
            while interval_stream.next().await.is_some() {
                if let Err(e) = ctrls.update_speeds().await {
                    error!("Update speed error: {e}");
                }

                info!("[timer] tick");
            }
        }
    });

    let stop = event_listener::Event::new();
    let stop_listener = stop.listen();
    let _conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at("/io/github/tt_riingd", DBusInterface {
            controllers,
            stop,
            version,
        })?
        .build()
        .await?;

    stop_listener.wait();
    info!("Stopped");

    Ok(())
}

fn main() -> Result<()> {
    into_daemon()
        .and_then(|_| init_log())
        .and_then(|_| tokio_main())
}
