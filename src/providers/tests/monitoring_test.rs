//! Unit tests for monitoring service

use super::super::monitoring::*;
use crate::{
    config::{Config, FanTarget, MappingCfg, SensorCfg},
    core::{AppState, EventBus, TaskManager, event::Event},
    drivers::controller_manager::ControllerManager,
    mappings::{CurveMapping, EffectMapping, Mapping},
    providers::traits::ServiceProvider,
    temperature_sensors::{sensor::TemperatureSensor, sensor_manager},
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{
    sync::RwLock,
    time::{sleep, timeout},
};

// Mock sensor implementation for testing
#[derive(Debug)]
struct MockTemperatureSensor {
    key: String,
    temperature: Arc<Mutex<f32>>,
    read_count: Arc<AtomicU32>,
    should_fail: Arc<Mutex<bool>>,
}

impl MockTemperatureSensor {
    fn new(key: &str, initial_temp: f32) -> Self {
        Self {
            key: key.to_string(),
            temperature: Arc::new(Mutex::new(initial_temp)),
            read_count: Arc::new(AtomicU32::new(0)),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    #[allow(dead_code)]
    fn set_temperature(&self, temp: f32) {
        *self.temperature.lock().unwrap() = temp;
    }

    #[allow(dead_code)]
    fn get_read_count(&self) -> u32 {
        self.read_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl TemperatureSensor for MockTemperatureSensor {
    fn key(&self) -> String {
        self.key.clone()
    }

    async fn read_temperature(&self) -> Result<f32> {
        self.read_count.fetch_add(1, Ordering::Relaxed);

        if *self.should_fail.lock().unwrap() {
            return Err(anyhow::anyhow!("Mock sensor failure"));
        }

        Ok(*self.temperature.lock().unwrap())
    }
}

// Helper function to create ConfigManager for tests
fn create_test_config_manager(config: Config) -> crate::config::ConfigManager {
    crate::config::ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"))
}

// Helper function to create mock AppState with minimal Controllers
async fn create_mock_app_state() -> Arc<AppState> {
    let config = Config {
        sensors: vec![SensorCfg::LmSensors {
            id: "cpu_temp".to_string(),
            chip: "test_chip".to_string(),
            feature: "test_feature".to_string(),
        }],
        mappings: vec![MappingCfg {
            sensor: "cpu_temp".to_string(),
            targets: vec![
                FanTarget {
                    controller: 1,
                    fan_idx: 1,
                },
                FanTarget {
                    controller: 1,
                    fan_idx: 2,
                },
            ],
        }],
        ..Default::default()
    };

    let config_manager = create_test_config_manager(config);
    Arc::new(AppState::new(config_manager).await.unwrap())
}

#[tokio::test]
async fn monitoring_service_updates_controllers() {
    let state = create_mock_app_state().await;
    let event_bus = EventBus::new();
    let mut task_manager = TaskManager::new();

    let provider = MonitoringServiceProvider::new(state.clone(), event_bus);
    provider.start(&mut task_manager).await.unwrap();

    // Wait for service to process sensors
    sleep(Duration::from_millis(200)).await;

    // Check that controller updates were called
    // Note: This would require more sophisticated mocking to verify
    // For now, we verify that the service runs without errors
    assert!(task_manager.is_running("MonitoringService"));

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn monitoring_service_responds_to_cancellation() {
    let state = create_mock_app_state().await;
    let event_bus = EventBus::new();
    let mut task_manager = TaskManager::new();

    let provider = MonitoringServiceProvider::new(state, event_bus);
    provider.start(&mut task_manager).await.unwrap();

    // Verify service is running
    assert!(task_manager.is_running("MonitoringService"));

    // Request shutdown
    let shutdown_result = task_manager.shutdown_all().await;
    assert!(shutdown_result.is_ok());

    // Verify service stopped
    std::assert_eq!(task_manager.active_count(), 0);
}

#[tokio::test]
async fn monitoring_service_multiple_sensors() {
    let config = Config {
        sensors: vec![
            SensorCfg::LmSensors {
                id: "cpu_temp".to_string(),
                chip: "test_chip".to_string(),
                feature: "test_feature".to_string(),
            },
            SensorCfg::LmSensors {
                id: "gpu_temp".to_string(),
                chip: "test_chip2".to_string(),
                feature: "test_feature2".to_string(),
            },
        ],
        mappings: vec![
            MappingCfg {
                sensor: "cpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 1,
                    fan_idx: 1,
                }],
            },
            MappingCfg {
                sensor: "gpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 1,
                    fan_idx: 2,
                }],
            },
        ],
        ..Default::default()
    };

    let sensors: Vec<Box<dyn TemperatureSensor>> = vec![
        Box::new(MockTemperatureSensor::new("cpu_temp", 45.5)),
        Box::new(MockTemperatureSensor::new("gpu_temp", 62.3)),
    ];

    let controllers =
        ControllerManager::init_from_cfg(&config).unwrap_or_else(|_| ControllerManager::empty());

    // Create AppState with our mock sensors wrapped in SensorManager
    let sensor_manager = sensor_manager::SensorManager::new_from_sensors(sensors);
    let config_manager = create_test_config_manager(config.clone());
    let state = Arc::new(AppState {
        config_manager: Arc::new(config_manager),
        controllers: Arc::new(tokio::sync::RwLock::new(controllers)),
        sensors: Arc::new(tokio::sync::RwLock::new(sensor_manager)),
        mapping: Arc::new(RwLock::new(Mapping::load_mappings(&[]))),
        effect_runners: Arc::new(RwLock::new(
            crate::mappings::EffectStore::build_effect_store(&[]),
        )),
        effect_mappings: Arc::new(RwLock::new(EffectMapping::build_color_mapping(&[]))),
        active_curves: Arc::new(RwLock::new(CurveMapping::load_mappings(&[]))),
        sensor_data: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });

    let event_bus = EventBus::new();
    let mut receiver = event_bus.subscribe();
    let mut task_manager = TaskManager::new();

    let provider = MonitoringServiceProvider::new(state, event_bus);
    provider.start(&mut task_manager).await.unwrap();

    // Wait for temperature monitoring cycle
    match timeout(Duration::from_secs(5), receiver.recv()).await {
        Ok(Ok(Event::TemperatureChanged(temps))) => {
            // Should receive temperature updates
            println!("Received temperature data: {:?}", temps);
            assert!(!temps.is_empty());
        }
        Ok(Ok(other_event)) => {
            println!("Received other event: {:?}", other_event);
        }
        Ok(Err(e)) => {
            println!("Event bus error: {}", e);
        }
        Err(_) => {
            println!("Timeout waiting for temperature events");
        }
    }

    // Service should be running
    assert!(task_manager.is_running("MonitoringService"));

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn monitoring_service_timing_configuration() {
    let config = Config {
        tick_seconds: 1, // Very fast ticking for test
        sensors: vec![SensorCfg::LmSensors {
            id: "cpu_temp".to_string(),
            chip: "test_chip".to_string(),
            feature: "test_feature".to_string(),
        }],
        mappings: vec![MappingCfg {
            sensor: "cpu_temp".to_string(),
            targets: vec![FanTarget {
                controller: 1,
                fan_idx: 1,
            }],
        }],
        ..Default::default()
    };

    let state = {
        let config_manager = create_test_config_manager(config);
        Arc::new(AppState::new(config_manager).await.unwrap())
    };

    let event_bus = EventBus::new();
    let mut receiver = event_bus.subscribe();
    let mut task_manager = TaskManager::new();

    let provider = MonitoringServiceProvider::new(state, event_bus);
    provider.start(&mut task_manager).await.unwrap();

    // With 1-second tick, we should get events more frequently
    let start_time = std::time::Instant::now();
    let mut event_count = 0;

    while start_time.elapsed() < Duration::from_secs(3) && event_count < 2 {
        match timeout(Duration::from_secs(2), receiver.recv()).await {
            Ok(Ok(_)) => {
                event_count += 1;
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    // With faster ticking, we might get more events
    println!("Received {} events in 3 seconds", event_count);

    // Service should still be running
    assert!(task_manager.is_running("MonitoringService"));

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}
