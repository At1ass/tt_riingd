mod controller;
mod interface;

use std::fs::File;

use anyhow::{Result, anyhow};
use daemonize::Daemonize;
use event_listener::Listener;
use log::{LevelFilter, info};
use syslog::{BasicLogger, Facility, Formatter3164};
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

#[tokio::main]
async fn tokio_main() -> Result<()> {
    let controllers = Controllers::init()?;

    // First set
    controllers
        .send_init()
        .await
        .and(controllers.set_pwm(DEFAULT_PERCENT).await)?;

    info!("Start — {DEFAULT_PERCENT} %",);

    let stop = event_listener::Event::new();
    let stop_listener = stop.listen();
    let _conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at("/io/github/tt_riingd", DBusInterface { controllers, stop })?
        .build()
        .await?;

    stop_listener.wait();
    info!("Stopped");

    Ok(())
}

fn main() -> Result<()> {
    init_log()
        .and_then(|_| into_daemon())
        .and_then(|_| tokio_main())
}
