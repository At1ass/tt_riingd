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

use std::{collections::HashMap, fs::File, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use clap::Parser;
use config::ColorCfg;
use daemonize::Daemonize;
use event_listener::Listener;
use log::{LevelFilter, error, info};
use mappings::{ColorMapping, Mapping};
use once_cell::sync::Lazy;
use sensors::TemperatureSensor;
use syslog::{BasicLogger, Facility, Formatter3164};
use temperature_sensors::lm_sensor;
use tokio::{sync::RwLock, task::JoinHandle, time::interval};
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use zbus::connection;

use interface::{DBusInterface, DBusInterfaceSignals};

pub struct AppContext {
    pub cfg: config::Config,
    pub controllers: controller::Controllers,
    pub sensors: Vec<Box<dyn TemperatureSensor>>,
    pub mapping: Arc<Mapping>,
    pub colors: Arc<Vec<ColorCfg>>,
    pub color_mappings: Arc<ColorMapping>,
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
    sensors_data: Arc<RwLock<HashMap<String, f32>>>,
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
                            sensors_data.write().await.insert(name.clone(), t);
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

fn spawn_broadcast_task(
    connection: zbus::Connection,
    sensors_data: Arc<RwLock<HashMap<String, f32>>>,
    broadcast_tick: u64,
) -> JoinHandle<()> {
    info!("Starting broadcast task with interval {broadcast_tick}");
    tokio::spawn({
        let mut interval_stream =
            IntervalStream::new(interval(Duration::from_secs(broadcast_tick)));
        let mut cache: HashMap<String, f32> = HashMap::new();
        async move {
            while interval_stream.next().await.is_some() {
                if let Ok(interface) = connection
                    .object_server()
                    .interface("/io/github/tt_riingd")
                    .await
                {
                    let snapshot = sensors_data.read().await.clone();
                    if (!(snapshot
                        .iter()
                        .any(|(s, t)| (t - cache.get(s).unwrap_or(t)).abs() >= 0.2))
                        && !cache.is_empty())
                        || snapshot.is_empty()
                    {
                        continue;
                    }

                    let _ = interface.temperature_changed(snapshot.clone()).await;
                    cache = snapshot;
                } else {
                    error!("Failed to get object server interface");
                    continue;
                }
                info!("[Color] tick");
            }
        }
    })
}

fn spawn_color_task(
    controllers: controller::Controllers,
    color_map: Arc<ColorMapping>,
    colors: Arc<Vec<ColorCfg>>,
) -> JoinHandle<()> {
    tokio::spawn({
        let mut interval_stream = IntervalStream::new(interval(Duration::from_secs(3)));
        async move {
            while interval_stream.next().await.is_some() {
                let map: Vec<_> = color_map
                    .iter()
                    .filter_map(|entry| {
                        colors
                            .iter()
                            .find(|&c| c.color == *entry.key())
                            .map(|finded| (finded, entry.value().clone()))
                    }).collect();
                for (cfg, fans) in map {
                    for fan in fans {
                        let value = controllers.clone();
                        let _ = value
                           .update_channel_color(
                                fan.controller_id as u8,
                                fan.channel as u8,
                                cfg.rgb[0],
                                cfg.rgb[1],
                                cfg.rgb[2],
                            )
                           .await;
                    }
                }
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
    let colors = Arc::new(config.colors.clone());
    let color_mappings = Arc::new(ColorMapping::build_color_mapping(&config.color_mappings));

    Ok(AppContext {
        cfg: config,
        controllers,
        sensors,
        mapping,
        colors,
        color_mappings,
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
    } = init_context(config_path).await?;

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
        Some(spawn_broadcast_task(
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
