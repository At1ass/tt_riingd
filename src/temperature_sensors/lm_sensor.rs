//! lm-sensors integration for hardware temperature monitoring.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;

use lm_sensors::{
    LMSensors, SubFeatureRef,
    value::{Kind as ValueKind, Value},
};

use crate::{config::SensorCfg, sensors::TemperatureSensor};

struct Sensor {
    key: String,
    subf: SubFeatureRef<'static>,
}

// SAFETY: libsensors (>= 3.6) guards all sensor access with an internal global mutex.
//         The `SubFeatureRef::value()` call is read-only.
//         Therefore, moving this pointer across threads cannot cause data races.
unsafe impl Send for Sensor {}
unsafe impl Sync for Sensor {}

/// Temperature sensor implementation using lm-sensors library.
///
/// Provides access to hardware temperature sensors through the lm-sensors
/// library with proper async handling of blocking operations.
pub struct LmSensorSource(Arc<Mutex<Sensor>>);

impl LmSensorSource {
    /// Discovers available temperature sensors from configuration.
    ///
    /// Scans the lm-sensors library for configured sensors and creates
    /// sensor instances for each valid configuration.
    pub fn discover(
        lmsensors: &'static LMSensors,
        cfg: &[SensorCfg],
    ) -> Vec<Box<dyn TemperatureSensor>> {
        cfg.iter()
            .filter_map(|c| {
                let SensorCfg::LmSensors { id, chip, feature } = c;
                #[cfg(debug_assertions)]
                {
                    log::info!("Discovering LM sensor: chip={chip}, feature={feature}");
                }
                let chip_ref = lmsensors
                    .chip_iter(None)
                    .find(|c| c.name().is_ok_and(|n| n == *chip))?;
                let feat_ref = chip_ref.feature_iter().find(|f| {
                    f.name()
                        .map(|n| n.unwrap_or("N/A"))
                        .is_some_and(|s| s == *feature)
                })?;
                let subfeat_ref = feat_ref
                    .sub_feature_iter()
                    .find(|s| matches!(s.kind(), Some(ValueKind::TemperatureInput)))?;

                #[cfg(debug_assertions)]
                {
                    let chip_name = chip_ref.name().unwrap_or("unknown".to_string());
                    let chip_bus = chip_ref.bus();
                    let feat_name = feat_ref
                        .name()
                        .map(|n| n.unwrap_or("unknown"))
                        .unwrap_or("unknown");
                    let sensor_key = format!("lm:{chip_name}@{chip_bus}:{feat_name}");
                    log::info!("Found LM sensor: {sensor_key}");
                }

                Some(Box::new(Self(Arc::new(Mutex::new(Sensor {
                    key: id.to_string(),
                    subf: subfeat_ref,
                })))) as Box<dyn TemperatureSensor>)
            })
            .collect::<Vec<_>>()
    }
}

#[async_trait]
impl TemperatureSensor for LmSensorSource {
    async fn read_temperature(&self) -> Result<f32> {
        let sensor = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let sensor = sensor
                .lock()
                .map_err(|e| anyhow::anyhow!("Mutex poisoned: {e}"))?;
            let value = sensor.subf.value()?;
            match value {
                #[allow(clippy::cast_possible_truncation)]
                Value::TemperatureInput(t) => Ok(t as f32),
                _ => Err(anyhow::anyhow!("Invalid temperature value type")),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Blocking task failed: {e}"))?
    }

    fn key(&self) -> String {
        self.0
            .lock()
            .map_or_else(|_| "unknown".to_string(), |s| s.key.clone())
    }
}
