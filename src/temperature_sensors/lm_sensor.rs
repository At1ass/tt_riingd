use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use lm_sensors::{
    LMSensors, SubFeatureRef,
    value::{Kind as ValueKind, Value},
};
#[cfg(debug_assertions)]
use log::info;
use tokio::sync::Mutex;

use crate::{config::SensorCfg, sensors::TemperatureSensor};

pub struct Sensor {
    key: String,
    subf: SubFeatureRef<'static>,
}

// SAFETY: libsensors (>= 3.6) guards all sensor access with an internal global mutex.
//         The `SubFeatureRef::value()` call is read-only.
//         Therefore, moving this pointer across threads cannot cause data races.
unsafe impl Send for Sensor {}
unsafe impl Sync for Sensor {}

pub struct LmSensorSource(Arc<Mutex<Sensor>>);

impl LmSensorSource {
    #[allow(unreachable_patterns)]
    pub fn discover(
        lmsensors: &'static LMSensors,
        cfg: &[SensorCfg],
    ) -> Result<Vec<Box<dyn TemperatureSensor>>> {
        Ok(cfg
            .iter()
            .filter_map(|c| match c {
                SensorCfg::LmSensors { id, chip, feature } => {
                    #[cfg(debug_assertions)]
                    {
                        info!("Discovering LM sensor: chip={}, feature={}", chip, feature);
                    }
                    let chip_ref = lmsensors
                        .chip_iter(None)
                        .find(|c| c.name().map(|n| n == *chip).unwrap_or(false))?;
                    let feat_ref = chip_ref.feature_iter().find(|f| {
                        f.name()
                            .map(|n| n.unwrap_or("N/A"))
                            .map(|s| s == *feature)
                            .unwrap_or(false)
                    })?;
                    let subfeat_ref = feat_ref
                        .sub_feature_iter()
                        .find(|s| matches!(s.kind(), Some(ValueKind::TemperatureInput)))?;

                    #[cfg(debug_assertions)]
                    {
                        let chip_name = chip_ref.name().unwrap();
                        let chip_bus = chip_ref.bus();
                        let feat_name = feat_ref.name()?.unwrap();
                        let sensor_key = format!("lm:{chip_name}@{chip_bus}:{feat_name}");
                        info!("Found LM sensor: {sensor_key}");
                    }

                    Some(Box::new(LmSensorSource(Arc::new(Mutex::new(Sensor {
                        key: id.to_string(),
                        subf: subfeat_ref,
                    })))) as Box<dyn TemperatureSensor>)
                }
                _ => None,
            })
            .collect::<Vec<_>>())
    }
}

#[async_trait]
impl TemperatureSensor for LmSensorSource {
    async fn sensor_name(&self) -> Option<String> {
        Some(self.0.lock().await.key.clone())
    }

    async fn read_temperature(&self) -> Result<f32> {
        match self.0.lock().await.subf.value()? {
            Value::TemperatureInput(t) => Ok(t as f32),
            _ => Err(anyhow!("non-temperature value")),
        }
    }
}
