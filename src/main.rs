mod app_context;
mod application;
mod cli;
mod config;
mod controller;
mod coordinator;
mod drivers;
mod event;
mod fan_controller;
mod fan_curve;
mod interface;
mod mappings;
mod providers;
mod sensors;
mod task_manager;
mod temperature_sensors;

use std::{fs::File, path::PathBuf};

use anyhow::{Result, anyhow};
use application::Application;
use clap::Parser;
use daemonize::Daemonize;
use log::LevelFilter;
use syslog::{BasicLogger, Facility, Formatter3164};

fn init_log() -> Result<()> {
    syslog::unix(Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: "tt_riingd".into(),
        pid: 0,
    })
    .map_err(|e| anyhow!("{e}"))
    .and_then(|logger| {
        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|_| log::set_max_level(LevelFilter::Info))
            .map_err(|e| anyhow!("{e}"))
    })
}

fn into_daemon(daemonize: bool) -> Result<()> {
    daemonize
        .then(|| {
            File::create("/var/tmp/tt_riingd.log")
                .and_then(|out| Ok((out.try_clone()?, out)))
                .map_err(|e| anyhow!("{e}"))
                .and_then(|(stderr, stdout)| {
                    Daemonize::new()
                        .pid_file("/tmp/tt_riingd.pid")
                        .stdout(stdout)
                        .stderr(stderr)
                        .start()
                        .map_err(|e| anyhow!("{e}"))
                })
        })
        .map_or(Ok(()), |res| res)
}

#[tokio::main]
async fn tokio_main(config_path: Option<PathBuf>) -> Result<()> {
    #[cfg(feature = "tokio-console")]
    {
        console_subscriber::init();
    }
    let config_manager = config::ConfigManager::load(config_path).await?;
    Application::builder()
        .with_config_manager(config_manager)
        .build()
        .await?
        .run()
        .await?;

    Ok(())
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    into_daemon(cli.daemonize)
        .and_then(|_| init_log())
        .and_then(|_| tokio_main(cli.config))
}
