use super::super::sensor_manager::SensorManager;
use crate::temperature_sensors::sensor::TemperatureSensor;
use anyhow::Result;
use async_trait::async_trait;

struct MockSensor {
    key: String,
    temperature: f32,
}

#[async_trait]
impl TemperatureSensor for MockSensor {
    async fn read_temperature(&self) -> Result<f32> {
        Ok(self.temperature)
    }

    fn key(&self) -> String {
        self.key.clone()
    }
}

#[test]
fn test_sensor_manager_basic_operations() {
    let sensors = vec![
        Box::new(MockSensor {
            key: "cpu_temp".to_string(),
            temperature: 45.0,
        }) as Box<dyn TemperatureSensor>,
        Box::new(MockSensor {
            key: "gpu_temp".to_string(),
            temperature: 60.0,
        }),
    ];

    let manager = SensorManager::new_from_sensors(sensors);

    std::assert_eq!(manager.len(), 2);
    assert!(!manager.is_empty());

    let keys: Vec<String> = manager.iter().map(|s| s.key()).collect();
    std::assert_eq!(keys, vec!["cpu_temp", "gpu_temp"]);

    let cpu_sensor = manager.find_by_key("cpu_temp");
    assert!(cpu_sensor.is_some());
    std::assert_eq!(cpu_sensor.unwrap().key(), "cpu_temp");

    let nonexistent = manager.find_by_key("nonexistent");
    assert!(nonexistent.is_none());
}
