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
/// use tt_riingd::sensors::TemperatureSensor;
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    // Mock sensor for successful operations
    struct MockSuccessfulSensor {
        key: String,
        temperature: f32,
    }

    #[async_trait]
    impl TemperatureSensor for MockSuccessfulSensor {
        async fn read_temperature(&self) -> Result<f32> {
            Ok(self.temperature)
        }

        fn key(&self) -> String {
            self.key.clone()
        }
    }

    // Mock sensor that always fails
    struct MockFailingSensor {
        key: String,
        error_message: String,
    }

    #[async_trait]
    impl TemperatureSensor for MockFailingSensor {
        async fn read_temperature(&self) -> Result<f32> {
            Err(anyhow!(self.error_message.clone()))
        }

        fn key(&self) -> String {
            self.key.clone()
        }
    }

    // Mock sensor with variable delay for async testing
    struct MockSlowSensor {
        key: String,
        temperature: f32,
        delay_ms: u64,
    }

    #[async_trait]
    impl TemperatureSensor for MockSlowSensor {
        async fn read_temperature(&self) -> Result<f32> {
            sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(self.temperature)
        }

        fn key(&self) -> String {
            self.key.clone()
        }
    }

    // Mock sensor with state tracking
    struct MockStatefulSensor {
        key: String,
        read_count: Arc<Mutex<usize>>,
        temperatures: Vec<f32>,
    }

    #[async_trait]
    impl TemperatureSensor for MockStatefulSensor {
        async fn read_temperature(&self) -> Result<f32> {
            let mut count = self.read_count.lock().unwrap();
            let index = *count % self.temperatures.len();
            *count += 1;
            Ok(self.temperatures[index])
        }

        fn key(&self) -> String {
            self.key.clone()
        }
    }

    #[tokio::test]
    async fn successful_sensor_read_temperature() {
        let sensor = MockSuccessfulSensor {
            key: "cpu_temp".to_string(),
            temperature: 65.5,
        };

        let result = sensor.read_temperature().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 65.5);
    }

    #[tokio::test]
    async fn successful_sensor_key() {
        let sensor = MockSuccessfulSensor {
            key: "gpu_temp".to_string(),
            temperature: 70.0,
        };

        assert_eq!(sensor.key(), "gpu_temp");
    }

    #[tokio::test]
    async fn failing_sensor_returns_error() {
        let sensor = MockFailingSensor {
            key: "broken_sensor".to_string(),
            error_message: "Hardware communication failed".to_string(),
        };

        let result = sensor.read_temperature().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Hardware communication failed")
        );
    }

    #[tokio::test]
    async fn failing_sensor_key_still_works() {
        let sensor = MockFailingSensor {
            key: "broken_sensor".to_string(),
            error_message: "Error".to_string(),
        };

        assert_eq!(sensor.key(), "broken_sensor");
    }

    #[tokio::test]
    async fn slow_sensor_async_behavior() {
        let sensor = MockSlowSensor {
            key: "slow_sensor".to_string(),
            temperature: 42.0,
            delay_ms: 50,
        };

        let start = std::time::Instant::now();
        let result = sensor.read_temperature().await;
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42.0);
        assert!(duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn concurrent_sensor_reads() {
        let sensor = Arc::new(MockSuccessfulSensor {
            key: "concurrent_sensor".to_string(),
            temperature: 55.0,
        });

        let mut handles = vec![];
        for _ in 0..10 {
            let sensor_clone = sensor.clone();
            let handle = tokio::spawn(async move { sensor_clone.read_temperature().await });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 55.0);
        }
    }

    #[tokio::test]
    async fn stateful_sensor_cycling_values() {
        let sensor = MockStatefulSensor {
            key: "cycling_sensor".to_string(),
            read_count: Arc::new(Mutex::new(0)),
            temperatures: vec![30.0, 40.0, 50.0],
        };

        // First cycle through values
        assert_eq!(sensor.read_temperature().await.unwrap(), 30.0);
        assert_eq!(sensor.read_temperature().await.unwrap(), 40.0);
        assert_eq!(sensor.read_temperature().await.unwrap(), 50.0);

        // Should cycle back to beginning
        assert_eq!(sensor.read_temperature().await.unwrap(), 30.0);
        assert_eq!(sensor.read_temperature().await.unwrap(), 40.0);
    }

    #[tokio::test]
    async fn sensor_trait_object_compatibility() {
        let sensors: Vec<Box<dyn TemperatureSensor>> = vec![
            Box::new(MockSuccessfulSensor {
                key: "sensor1".to_string(),
                temperature: 30.0,
            }),
            Box::new(MockSuccessfulSensor {
                key: "sensor2".to_string(),
                temperature: 40.0,
            }),
        ];

        for sensor in &sensors {
            let result = sensor.read_temperature().await;
            assert!(result.is_ok());
            assert!(sensor.key().starts_with("sensor"));
        }
    }

    #[tokio::test]
    async fn extreme_temperature_values() {
        let extreme_sensors = vec![
            MockSuccessfulSensor {
                key: "very_cold".to_string(),
                temperature: -273.15, // Absolute zero
            },
            MockSuccessfulSensor {
                key: "very_hot".to_string(),
                temperature: 1000.0, // Very high temperature
            },
            MockSuccessfulSensor {
                key: "zero".to_string(),
                temperature: 0.0,
            },
        ];

        for sensor in extreme_sensors {
            let result = sensor.read_temperature().await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn sensor_error_types() {
        let error_sensors = vec![
            MockFailingSensor {
                key: "io_error".to_string(),
                error_message: "I/O error reading sensor".to_string(),
            },
            MockFailingSensor {
                key: "permission_error".to_string(),
                error_message: "Permission denied".to_string(),
            },
            MockFailingSensor {
                key: "hardware_error".to_string(),
                error_message: "Hardware not found".to_string(),
            },
        ];

        for sensor in error_sensors {
            let result = sensor.read_temperature().await;
            assert!(result.is_err());
            let error_str = result.unwrap_err().to_string();
            assert!(!error_str.is_empty());
        }
    }

    #[tokio::test]
    async fn sensor_key_uniqueness() {
        let sensors = vec![
            MockSuccessfulSensor {
                key: "cpu_temp".to_string(),
                temperature: 50.0,
            },
            MockSuccessfulSensor {
                key: "gpu_temp".to_string(),
                temperature: 60.0,
            },
            MockSuccessfulSensor {
                key: "motherboard_temp".to_string(),
                temperature: 40.0,
            },
        ];

        let mut keys = std::collections::HashSet::new();
        for sensor in sensors {
            let key = sensor.key();
            assert!(!keys.contains(&key), "Duplicate key found: {}", key);
            keys.insert(key);
        }

        assert_eq!(keys.len(), 3);
    }

    #[tokio::test]
    async fn concurrent_mixed_sensors() {
        let sensors: Vec<Box<dyn TemperatureSensor>> = vec![
            Box::new(MockSuccessfulSensor {
                key: "fast_sensor".to_string(),
                temperature: 45.0,
            }),
            Box::new(MockSlowSensor {
                key: "slow_sensor".to_string(),
                temperature: 55.0,
                delay_ms: 10,
            }),
            Box::new(MockFailingSensor {
                key: "failing_sensor".to_string(),
                error_message: "Sensor failure".to_string(),
            }),
        ];

        let mut handles = vec![];
        for sensor in sensors {
            let handle = tokio::spawn(async move { sensor.read_temperature().await });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        // First two should work, last should fail
        assert!(results[0].as_ref().unwrap().is_ok());
        assert!(results[1].as_ref().unwrap().is_ok());
        assert!(results[2].as_ref().unwrap().is_err());
    }
}
