//! lm-sensors integration for hardware temperature monitoring.

use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, info, warn};

use lm_sensors::{
    SubFeatureRef,
    value::{Kind as ValueKind, Value},
};

use crate::{config::SensorCfg, temperature_sensors::sensor::TemperatureSensor};

struct Sensor {
    key: String,
    subf: SubFeatureRef<'static>,
}
unsafe impl Send for Sensor {}
unsafe impl Sync for Sensor {}

/// Wrapper for lm-sensors library instance.
///
/// This wrapper is needed to implement Send + Sync for the lm-sensors
/// library which doesn't implement these traits by default.
pub struct LMSensorsRef(pub lm_sensors::LMSensors);

// SAFETY: lm-sensors library (>= 3.6) uses internal global mutex for all operations.
// The library is thread-safe but doesn't implement Send/Sync markers.
unsafe impl Send for LMSensorsRef {}
unsafe impl Sync for LMSensorsRef {}

/// Global lm-sensors instance.
///
/// Initialized once at startup and shared across all temperature sensor instances.
/// Uses LazyLock for thread-safe lazy initialization.
/// Returns None if lm-sensors is not available on the system.
pub static LMSENSORS: LazyLock<Option<LMSensorsRef>> =
    LazyLock::new(|| match lm_sensors::Initializer::default().initialize() {
        Ok(sensors) => {
            info!("lm-sensors initialized successfully");
            Some(LMSensorsRef(sensors))
        }
        Err(e) => {
            warn!(
                "lm-sensors not available: {}. Temperature monitoring will be limited.",
                e
            );
            None
        }
    });

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
        // lmsensors: &'static LMSensors,
        cfg: &[SensorCfg],
    ) -> Vec<Box<dyn TemperatureSensor>> {
        cfg.iter()
            .filter_map(|c| {
                if let SensorCfg::LmSensors { id, chip, feature } = c {
                    #[cfg(debug_assertions)]
                    {
                        debug!("Discovering LM sensor: chip={chip}, feature={feature}");
                    }
                    let chip_ref = LMSENSORS
                        .as_ref()?
                        .0
                        .chip_iter(None)
                        .find(|c| c.name().is_ok_and(|n| n == *chip))
                        .or_else(|| {
                            warn!(
                                "Chip '{chip}' not found in lm-sensors. Skipping sensor discovery."
                            );
                            None
                        })?;
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
                        debug!("Found LM sensor: {sensor_key}");
                    }

                    Some(Box::new(Self(Arc::new(Mutex::new(Sensor {
                        key: id.to_string(),
                        subf: subfeat_ref,
                    })))) as Box<dyn TemperatureSensor>)
                } else {
                    None
                }
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
