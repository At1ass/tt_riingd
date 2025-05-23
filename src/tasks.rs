use std::{collections::HashMap, sync::Arc, time::Duration};

#[cfg(debug_assertions)]
use log::info;
use log::error;
use tokio::{sync::RwLock, task::JoinHandle, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};

use crate::{config::ColorCfg, controller, interface::DBusInterfaceSignals, mappings::{ColorMapping, Mapping}, sensors::TemperatureSensor};

pub fn spawn_monitoring_task(
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
                            #[cfg(debug_assertions)]
                            {
                                info!("Temperature of {name}: {t}°C");
                            }
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
                #[cfg(debug_assertions)]
                {
                    info!("[timer] tick");
                }
            }
        }
    })
}

pub fn spawn_broadcast_task(
    connection: zbus::Connection,
    sensors_data: Arc<RwLock<HashMap<String, f32>>>,
    broadcast_tick: u64,
) -> JoinHandle<()> {
    #[cfg(debug_assertions)]
    {
        info!("Starting broadcast task with interval {broadcast_tick}");
    }

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
                #[cfg(debug_assertions)]
                {
                    info!("[timer] tick");
                }
            }
        }
    })
}

pub fn spawn_color_task(
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
                    })
                    .collect();
                for (cfg, fans) in map {
                    for fan in fans {
                        let ret = controllers
                            .update_channel_color(
                                fan.controller_id as u8,
                                fan.channel as u8,
                                cfg.rgb[0],
                                cfg.rgb[1],
                                cfg.rgb[2],
                            )
                            .await;
                        if let Err(e) = ret {
                            error!("update_channel_color error: {e}");
                        }
                    }
                }
            }
        }
    })
}
