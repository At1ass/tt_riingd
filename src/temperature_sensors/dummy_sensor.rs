use anyhow::Result;
use async_trait::async_trait;

use crate::temperature_sensors::sensor::TemperatureSensor;

pub struct DummySensorSource {
    key: String,
    temperature: f32,
}

impl DummySensorSource {
    pub fn new(key: String, temperature: f32) -> Self {
        Self { key, temperature }
    }

    pub fn discover() -> Vec<Box<dyn TemperatureSensor>> {
        vec![
            Box::new(DummySensorSource::new("dummy_sensor".to_string(), 45.0))
                as Box<dyn TemperatureSensor>,
        ]
    }
}

#[async_trait]
impl TemperatureSensor for DummySensorSource {
    async fn read_temperature(&self) -> Result<f32> {
        Ok(self.temperature)
    }

    fn key(&self) -> String {
        self.key.clone()
    }
}
