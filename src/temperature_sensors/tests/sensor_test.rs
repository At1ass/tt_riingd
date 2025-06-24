//! Unit tests for temperature sensor functionality
//!
//! Tests sensor reading, error handling, and various sensor types

use super::super::*;
use anyhow::Result;
use std::assert_eq;
use proptest::prelude::*;

/// Test fixtures for sensor testing
mod fixtures {
    use super::*;

    pub struct MockSensor {
        pub id: String,
        pub temperature: f32,
        pub should_fail: bool,
        pub read_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MockSensor {
        pub fn new(id: &str, temperature: f32) -> Self {
            Self {
                id: id.to_string(),
                temperature,
                should_fail: false,
                read_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        pub fn new_failing(id: &str) -> Self {
            Self {
                id: id.to_string(),
                temperature: 0.0,
                should_fail: true,
                read_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        pub fn get_read_count(&self) -> usize {
            self.read_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl TemperatureSensor for MockSensor {
        fn read_temperature(&self) -> Result<f32> {
            self.read_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            
            if self.should_fail {
                return Err(anyhow::anyhow!("Sensor read failed"));
            }
            
            Ok(self.temperature)
        }

        fn get_id(&self) -> &str {
            &self.id
        }

        fn is_available(&self) -> bool {
            !self.should_fail
        }
    }
}

/// Tests for TemperatureSensor trait
mod temperature_sensor_tests {
    use super::*;

    #[test]
    fn test_temperature_sensor_successful_read() {
        let sensor = fixtures::MockSensor::new("test_sensor", 45.5);
        
        let result = sensor.read_temperature();
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 45.5);
        std::assert_eq!(sensor.get_read_count(), 1);
    }

    #[test]
    fn test_temperature_sensor_failed_read() {
        let sensor = fixtures::MockSensor::new_failing("failing_sensor");
        
        let result = sensor.read_temperature();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Sensor read failed"));
        std::assert_eq!(sensor.get_read_count(), 1);
    }

    #[test]
    fn test_temperature_sensor_multiple_reads() {
        let sensor = fixtures::MockSensor::new("multi_read_sensor", 37.2);
        
        for i in 1..=5 {
            let result = sensor.read_temperature();
            assert!(result.is_ok());
            std::assert_eq!(result.unwrap(), 37.2);
            std::assert_eq!(sensor.get_read_count(), i);
        }
    }

    #[test]
    fn test_temperature_sensor_id() {
        let sensor = fixtures::MockSensor::new("id_test_sensor", 25.0);
        std::assert_eq!(sensor.get_id(), "id_test_sensor");
    }

    #[test]
    fn test_temperature_sensor_availability() {
        let working_sensor = fixtures::MockSensor::new("working", 30.0);
        assert!(working_sensor.is_available());
        
        let failing_sensor = fixtures::MockSensor::new_failing("failing");
        assert!(!failing_sensor.is_available());
    }

    #[test]
    fn test_temperature_sensor_zero_temperature() {
        let sensor = fixtures::MockSensor::new("zero_temp", 0.0);
        
        let result = sensor.read_temperature();
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 0.0);
    }

    #[test]
    fn test_temperature_sensor_negative_temperature() {
        let sensor = fixtures::MockSensor::new("negative_temp", -10.5);
        
        let result = sensor.read_temperature();
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), -10.5);
    }

    #[test]
    fn test_temperature_sensor_high_temperature() {
        let sensor = fixtures::MockSensor::new("high_temp", 100.0);
        
        let result = sensor.read_temperature();
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 100.0);
    }

    #[test]
    fn test_temperature_sensor_precision() {
        let sensor = fixtures::MockSensor::new("precise_temp", 36.742);
        
        let result = sensor.read_temperature();
        assert!(result.is_ok());
        assert!((result.unwrap() - 36.742).abs() < f32::EPSILON);
    }
}

/// Property-based tests for temperature sensors
mod property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_temperature_sensor_any_valid_temperature(temp in -50.0f32..150.0f32) {
            let sensor = fixtures::MockSensor::new("prop_test", temp);
            let result = sensor.read_temperature();
            
            assert!(result.is_ok());
            let read_temp = result.unwrap();
            assert!((read_temp - temp).abs() < f32::EPSILON);
        }

        #[test]
        fn test_temperature_sensor_id_consistency(id in "[a-zA-Z0-9_]{1,20}") {
            let sensor = fixtures::MockSensor::new(&id, 25.0);
            std::assert_eq!(sensor.get_id(), id);
        }

        #[test]
        fn test_temperature_sensor_multiple_reads_consistency(
            temp in -100.0f32..200.0f32,
            read_count in 1usize..10
        ) {
            let sensor = fixtures::MockSensor::new("consistency_test", temp);
            
            for _ in 0..read_count {
                let result = sensor.read_temperature();
                assert!(result.is_ok());
                assert!((result.unwrap() - temp).abs() < f32::EPSILON);
            }
            
            std::assert_eq!(sensor.get_read_count(), read_count);
        }
    }
}

/// Performance tests for temperature sensors
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_temperature_sensor_read_performance() {
        let sensor = fixtures::MockSensor::new("perf_test", 42.0);
        let iterations = 10000;
        
        let start = Instant::now();
        
        for _ in 0..iterations {
            let _ = sensor.read_temperature().unwrap();
        }
        
        let duration = start.elapsed();
        println!("Read temperature {} times in {:?}", iterations, duration);
        
        // Should complete reasonably fast
        assert!(duration.as_millis() < 1000);
        std::assert_eq!(sensor.get_read_count(), iterations);
    }

    #[test]
    fn test_temperature_sensor_concurrent_reads() {
        use std::sync::Arc;
        
        let sensor = Arc::new(fixtures::MockSensor::new("concurrent_test", 35.5));
        let thread_count = 10;
        let reads_per_thread = 100;
        
        let start = Instant::now();
        
        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                let sensor_clone = sensor.clone();
                std::thread::spawn(move || {
                    for _ in 0..reads_per_thread {
                        let result = sensor_clone.read_temperature();
                        assert!(result.is_ok());
                        std::assert_eq!(result.unwrap(), 35.5);
                    }
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let duration = start.elapsed();
        let total_reads = thread_count * reads_per_thread;
        
        println!("Performed {} concurrent reads in {:?}", total_reads, duration);
        std::assert_eq!(sensor.get_read_count(), total_reads);
        
        // Should complete reasonably fast
        assert!(duration.as_millis() < 2000);
    }

    #[test]
    fn test_sensor_availability_check_performance() {
        let working_sensor = fixtures::MockSensor::new("working", 25.0);
        let failing_sensor = fixtures::MockSensor::new_failing("failing");
        
        let iterations = 100000;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let _ = working_sensor.is_available();
            let _ = failing_sensor.is_available();
        }
        
        let duration = start.elapsed();
        println!("Checked availability {} times in {:?}", iterations * 2, duration);
        
        // Should complete very fast since it's just a boolean check
        assert!(duration.as_millis() < 100);
    }
}

/// Error handling tests
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_sensor_intermittent_failures() {
        struct IntermittentSensor {
            id: String,
            fail_every: usize,
            read_count: std::sync::atomic::AtomicUsize,
        }

        impl IntermittentSensor {
            fn new(id: &str, fail_every: usize) -> Self {
                Self {
                    id: id.to_string(),
                    fail_every,
                    read_count: std::sync::atomic::AtomicUsize::new(0),
                }
            }
        }

        impl TemperatureSensor for IntermittentSensor {
            fn read_temperature(&self) -> Result<f32> {
                let count = self.read_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                
                if count % self.fail_every == 0 {
                    Err(anyhow::anyhow!("Intermittent failure"))
                } else {
                    Ok(25.0 + (count as f32 * 0.1))
                }
            }

            fn get_id(&self) -> &str {
                &self.id
            }

            fn is_available(&self) -> bool {
                true // Always report as available
            }
        }

        let sensor = IntermittentSensor::new("intermittent", 3);
        
        // First read should fail (count = 0, 0 % 3 == 0)
        assert!(sensor.read_temperature().is_err());
        
        // Next two should succeed
        assert!(sensor.read_temperature().is_ok());
        assert!(sensor.read_temperature().is_ok());
        
        // Fourth read should fail (count = 3, 3 % 3 == 0)
        assert!(sensor.read_temperature().is_err());
    }

    #[test]
    fn test_sensor_error_messages() {
        struct CustomErrorSensor {
            error_msg: String,
        }

        impl TemperatureSensor for CustomErrorSensor {
            fn read_temperature(&self) -> Result<f32> {
                Err(anyhow::anyhow!("{}", self.error_msg))
            }

            fn get_id(&self) -> &str {
                "error_sensor"
            }

            fn is_available(&self) -> bool {
                false
            }
        }

        let test_cases = vec![
            "Hardware not found",
            "Permission denied",
            "Sensor disconnected",
            "Invalid data received",
            "Timeout occurred",
        ];

        for error_msg in test_cases {
            let sensor = CustomErrorSensor {
                error_msg: error_msg.to_string(),
            };
            
            let result = sensor.read_temperature();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains(error_msg));
        }
    }

    #[test]
    fn test_sensor_extreme_values() {
        let test_cases = vec![
            f32::MIN,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for temp in test_cases {
            let sensor = fixtures::MockSensor::new("extreme", temp);
            let result = sensor.read_temperature();
            
            if temp.is_finite() {
                assert!(result.is_ok());
                std::assert_eq!(result.unwrap(), temp);
            } else {
                // For infinite values, just ensure it doesn't panic
                let _ = result;
            }
        }
    }

    #[test]
    fn test_sensor_nan_handling() {
        let sensor = fixtures::MockSensor::new("nan_test", f32::NAN);
        let result = sensor.read_temperature();
        
        assert!(result.is_ok());
        assert!(result.unwrap().is_nan());
    }
}

/// Edge case tests
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_sensor_empty_id() {
        let sensor = fixtures::MockSensor::new("", 25.0);
        std::assert_eq!(sensor.get_id(), "");
    }

    #[test]
    fn test_sensor_very_long_id() {
        let long_id = "a".repeat(1000);
        let sensor = fixtures::MockSensor::new(&long_id, 25.0);
        std::assert_eq!(sensor.get_id(), long_id);
    }

    #[test]
    fn test_sensor_special_character_id() {
        let special_id = "sensor-with_special.chars@123!";
        let sensor = fixtures::MockSensor::new(special_id, 25.0);
        std::assert_eq!(sensor.get_id(), special_id);
    }

    #[test]
    fn test_sensor_unicode_id() {
        let unicode_id = "传感器_センサー_🌡️";
        let sensor = fixtures::MockSensor::new(unicode_id, 25.0);
        std::assert_eq!(sensor.get_id(), unicode_id);
    }

    #[test]
    fn test_sensor_state_changes() {
        struct StatefulSensor {
            id: String,
            temperatures: Vec<f32>,
            index: std::sync::atomic::AtomicUsize,
        }

        impl StatefulSensor {
            fn new(id: &str, temperatures: Vec<f32>) -> Self {
                Self {
                    id: id.to_string(),
                    temperatures,
                    index: std::sync::atomic::AtomicUsize::new(0),
                }
            }
        }

        impl TemperatureSensor for StatefulSensor {
            fn read_temperature(&self) -> Result<f32> {
                let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                
                if idx < self.temperatures.len() {
                    Ok(self.temperatures[idx])
                } else {
                    Err(anyhow::anyhow!("No more temperatures available"))
                }
            }

            fn get_id(&self) -> &str {
                &self.id
            }

            fn is_available(&self) -> bool {
                let idx = self.index.load(std::sync::atomic::Ordering::SeqCst);
                idx < self.temperatures.len()
            }
        }

        let temps = vec![20.0, 25.0, 30.0, 35.0];
        let sensor = StatefulSensor::new("stateful", temps.clone());
        
        // Read all available temperatures
        for expected_temp in temps {
            assert!(sensor.is_available());
            let result = sensor.read_temperature();
            assert!(result.is_ok());
            std::assert_eq!(result.unwrap(), expected_temp);
        }
        
        // Should not be available anymore
        assert!(!sensor.is_available());
        
        // Further reads should fail
        let result = sensor.read_temperature();
        assert!(result.is_err());
    }
}

/// Integration tests
mod integration_tests {
    use super::*;

    #[test]
    fn test_sensor_collection() {
        let sensors: Vec<Box<dyn TemperatureSensor>> = vec![
            Box::new(fixtures::MockSensor::new("cpu", 45.0)),
            Box::new(fixtures::MockSensor::new("gpu", 67.0)),
            Box::new(fixtures::MockSensor::new("ambient", 22.0)),
        ];

        let mut readings = Vec::new();
        
        for sensor in &sensors {
            if sensor.is_available() {
                match sensor.read_temperature() {
                    Ok(temp) => readings.push((sensor.get_id().to_string(), temp)),
                    Err(e) => eprintln!("Failed to read {}: {}", sensor.get_id(), e),
                }
            }
        }

        std::assert_eq!(readings.len(), 3);
        std::assert_eq!(readings[0], ("cpu".to_string(), 45.0));
        std::assert_eq!(readings[1], ("gpu".to_string(), 67.0));
        std::assert_eq!(readings[2], ("ambient".to_string(), 22.0));
    }

    #[test]
    fn test_sensor_with_retry_logic() {
        fn read_with_retry<T: TemperatureSensor>(
            sensor: &T,
            max_retries: usize,
        ) -> Result<f32> {
            let mut last_error = None;
            
            for attempt in 0..=max_retries {
                match sensor.read_temperature() {
                    Ok(temp) => return Ok(temp),
                    Err(e) => {
                        last_error = Some(e);
                        if attempt < max_retries {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                }
            }
            
            Err(last_error.unwrap())
        }

        // Test with working sensor
        let working_sensor = fixtures::MockSensor::new("working", 30.0);
        let result = read_with_retry(&working_sensor, 3);
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 30.0);

        // Test with failing sensor
        let failing_sensor = fixtures::MockSensor::new_failing("failing");
        let result = read_with_retry(&failing_sensor, 3);
        assert!(result.is_err());
        
        // Should have tried 4 times (initial + 3 retries)
        std::assert_eq!(failing_sensor.get_read_count(), 4);
    }
} 