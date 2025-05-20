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

use std::{fs::File, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use clap::Parser;
use daemonize::Daemonize;
use event_listener::Listener;
use log::{LevelFilter, error, info};
use mappings::Mapping;
use once_cell::sync::Lazy;
use sensors::TemperatureSensor;
use syslog::{BasicLogger, Facility, Formatter3164};
use temperature_sensors::lm_sensor;
use tokio::{task::JoinHandle, time::interval};
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use zbus::connection;

use interface::DBusInterface;

pub struct AppContext {
    pub cfg: config::Config,
    pub controllers: controller::Controllers,
    pub sensors: Vec<Box<dyn TemperatureSensor>>,
    pub mapping: Arc<Mapping>,
}

pub struct LMSensorsRef(pub lm_sensors::LMSensors);

unsafe impl Sync for LMSensorsRef {}
unsafe impl Send for LMSensorsRef {}

pub static LMSENSORS: Lazy<LMSensorsRef> = Lazy::new(|| {
    LMSensorsRef(
        lm_sensors::Initializer::default()
            .initialize()
            .expect("Cannot initialize lm-sensors"),
    )
});

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

fn spawn_monitoring_task(
    tick_seconds: u64,
    controllers: controller::Controllers,
    sensors: Vec<Box<dyn TemperatureSensor>>,
    mapping: Arc<Mapping>,
) -> JoinHandle<()> {
    tokio::spawn({
        let mut interval_stream = IntervalStream::new(interval(Duration::from_secs(tick_seconds)));
        async move {
            while interval_stream.next().await.is_some() {
                for sensor in &sensors {
                    let temp = sensor.read_temperature().await;

                    match temp {
                        Ok(t) => {
                            let Some(name) = sensor.sensor_name().await else {
                                continue;
                            };
                            info!("Temperature of {name}: {t}°C");
                            for fan in mapping.fans_for_sensor(&name) {
                                if let Err(e) = controllers
                                    .update_channel(fan.controller_id as u8, fan.channel as u8, t)
                                    .await
                                {
                                    error!("update_channel error: {e}");
                                }
                            }
                        }
                        Err(e) => error!("Temperature read error: {e}"),
                    }
                }
                info!("[timer] tick");
            }
        }
    })
}

async fn init_context(config_path: Option<PathBuf>) -> Result<AppContext> {
    let config = config::load(config_path)?;
    let controllers = controller::Controllers::init_from_cfg(&config)?;
    let sensors = lm_sensor::LmSensorSource::discover(&LMSENSORS.0, &config.sensors)?;

    info!("Loaded {} temperature sensors", sensors.len());

    let mapping = Arc::new(Mapping::load_mappings(&config.mappings));

    Ok(AppContext {
        cfg: config,
        controllers,
        sensors,
        mapping,
    })
}

#[tokio::main]
async fn tokio_main(config_path: Option<PathBuf>) -> Result<()> {
    let AppContext {
        cfg,
        controllers,
        sensors,
        mapping,
    } = init_context(config_path).await?;

    // First set
    controllers.send_init().await?;

    let _timer = spawn_monitoring_task(
        cfg.tick_seconds as u64,
        controllers.clone(),
        sensors,
        mapping,
    );

    let stop = event_listener::Event::new();
    let stop_listener = stop.listen();
    let _conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at(
            "/io/github/tt_riingd",
            DBusInterface {
                controllers,
                stop,
                version: cfg.version.to_string(),
            },
        )?
        .build()
        .await?;

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
