use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::{
    config::Config,
    temperature_sensors::sensor::TemperatureSensor,
    temperature_sensors::{lm_sensor::LmSensorSource, nvidia::NvidiaSensor},
};

/// Simple sensor manager that abstracts initialization and provides iteration.
///
/// Focuses on simplicity and performance - just what's needed for temperature monitoring.
pub struct SensorManager(Arc<Vec<Box<dyn TemperatureSensor>>>);

impl SensorManager {
    /// Creates a new SensorManager with the given temperature sensors.
    pub fn init_from_cfg(config: &Config) -> Result<Self> {
        let mut sensors = Vec::<Box<dyn TemperatureSensor>>::new();

        // Discover LM sensors
        sensors.extend(LmSensorSource::discover(&config.sensors));

        // Discover NVIDIA sensors
        sensors.extend(NvidiaSensor::discover(&config.sensors));

        info!("Initialized {} temperature sensors", sensors.len());
        Ok(Self(Arc::new(sensors)))
    }

    /// Creates a new SensorManager from a vector of sensors (for testing).
    #[allow(dead_code)]
    pub(crate) fn new_from_sensors(sensors: Vec<Box<dyn TemperatureSensor>>) -> Self {
        Self(Arc::new(sensors))
    }

    /// Returns an iterator over all managed sensors.
    pub fn iter(&self) -> impl Iterator<Item = &dyn TemperatureSensor> {
        self.0.iter().map(|s| s.as_ref() as &dyn TemperatureSensor)
    }

    /// Returns the number of sensors.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if no sensors are managed.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Finds a sensor by key (useful for debugging/logging).
    #[allow(dead_code)]
    pub fn find_by_key(&self, key: &str) -> Option<&dyn TemperatureSensor> {
        self.iter().find(|sensor| sensor.key() == key)
    }
}

#[cfg(test)]
#[path = "tests/sensor_manager_test.rs"]
mod sensor_manager_test;
