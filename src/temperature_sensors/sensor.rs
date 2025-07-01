//! Temperature sensor abstraction and implementations.
//!
//! Provides a unified interface for reading temperature data from various
//! sensor sources including lm-sensors and other hardware monitoring systems.

use anyhow::Result;
use async_trait::async_trait;

/// Trait for temperature sensor implementations.
///
/// Provides a unified interface for reading temperature data from various
/// hardware monitoring sources. All implementations must be thread-safe
/// and support async operations.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::temperature_sensors::sensor::TemperatureSensor;
/// use anyhow::Result;
///
/// struct MockSensor;
///
/// #[async_trait::async_trait]
/// impl TemperatureSensor for MockSensor {
///     async fn read_temperature(&self) -> Result<f32> {
///         Ok(42.5) // Mock temperature reading
///     }
///
///     fn key(&self) -> String {
///         "mock_sensor".to_string()
///     }
/// }
/// ```
#[async_trait]
pub trait TemperatureSensor: Send + Sync {
    /// Reads the current temperature from the sensor.
    ///
    /// Returns temperature in degrees Celsius or an error if reading fails.
    async fn read_temperature(&self) -> Result<f32>;

    /// Returns a unique identifier for this sensor.
    ///
    /// Used for mapping sensors to fan controllers and logging.
    fn key(&self) -> String;
}
