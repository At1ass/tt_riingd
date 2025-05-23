mod cli;
mod config;
mod controller;
mod drivers;
mod fan_controller;
mod fan_curve;
mod interface;
mod mappings;
mod sensors;
mod temperature_sensors;
mod tasks;
mod app_context;

use std::{collections::HashMap, fs::File, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use app_context::AppContext;
use clap::Parser;
use daemonize::Daemonize;
use event_listener::Listener;
use log::{LevelFilter, info};
use syslog::{BasicLogger, Facility, Formatter3164};
use tasks::{spawn_color_task, spawn_monitoring_task};
use tokio::sync::RwLock;
use zbus::connection;

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
async fn tokio_main(config_path: Option<PathBuf>) -> Result<()> {
    #[cfg(feature = "tokio-console")]
    {
        console_subscriber::init();
    }
    let AppContext {
        cfg,
        controllers,
        sensors,
        mapping,
        colors,
        color_mappings,
    } = app_context::init_context(config_path).await?;

    // First set
    controllers.send_init().await?;

    let stop = event_listener::Event::new();
    let stop_listener = stop.listen();

    let conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at(
            "/io/github/tt_riingd",
            DBusInterface {
                controllers: controllers.clone(),
                stop,
                version: cfg.version.to_string(),
            },
        )?
        .build()
        .await?;

    let _color = spawn_color_task(controllers.clone(), color_mappings.clone(), colors.clone());

    let sensors_data = Arc::new(RwLock::new(HashMap::new()));
    let _timer = spawn_monitoring_task(
        sensors_data.clone(),
        cfg.tick_seconds as u64,
        controllers,
        sensors,
        mapping,
    );

    let _broadcast = if cfg.enable_broadcast {
        Some(tasks::spawn_broadcast_task(
            conn.clone(),
            sensors_data.clone(),
            cfg.broadcast_interval as u64,
        ))
    } else {
        None
    };

    stop_listener.wait();
    info!("Stopped");

    Ok(())
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    into_daemon()
        .and_then(|_| init_log())
        .and_then(|_| tokio_main(cli.config))
}
