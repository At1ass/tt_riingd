use anyhow::Result;
use async_trait::async_trait;
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::{
    app_context::AppState,
    event::{Event, EventBus},
    providers::traits::ServiceProvider,
    task_manager::TaskManager,
};

/// Temperature monitoring service provider.
///
/// Provides a critical service that continuously monitors temperature sensors
/// and updates fan speeds based on configured curves and mappings. This is
/// the core service responsible for automatic fan control.
///
/// # Priority and Criticality
///
/// - **Priority**: 10 (highest)
/// - **Critical**: Yes (system cannot function without it)
///
/// # Features
///
/// - Periodic temperature sensor reading
/// - Automatic fan speed adjustment based on curves
/// - Temperature event publishing for other services
/// - Sensor failure handling and logging
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::MonitoringServiceProvider;
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = MonitoringServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub struct MonitoringServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
}

impl MonitoringServiceProvider {
    /// Creates a new monitoring service provider.
    pub fn new(state: Arc<AppState>, event_bus: EventBus) -> Self {
        Self { state, event_bus }
    }
}

#[async_trait]
impl ServiceProvider for MonitoringServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_monitoring_service(state, event_bus, cancel_token).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "MonitoringService"
    }

    fn priority(&self) -> i32 {
        10
    }

    fn is_critical(&self) -> bool {
        true
    }
}

async fn run_monitoring_service(
    state: Arc<AppState>,
    event_bus: EventBus,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut interval = interval(Duration::from_secs(u64::from(
        state.config().await.tick_seconds,
    )));

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Monitoring service cancelled");
                break;
            }
            _instant = interval.tick() => {
                if let Err(e) = collect_and_process_temperatures(&state, &event_bus).await {
                    log::error!("Failed to collect temperatures: {e}");
                }
            }
        }
    }
    Ok(())
}

async fn collect_and_process_temperatures(
    state: &Arc<AppState>,
    event_bus: &EventBus,
) -> Result<()> {
    let mut temperatures = HashMap::new();

    let sensors = state.sensors.read().await;
    for sensor in sensors.iter() {
        match sensor.read_temperature().await {
            Ok(temp) => {
                let sensor_name = sensor.key();
                temperatures.insert(sensor_name.clone(), temp);
                info!("Temperature of {sensor_name}: {temp:.2}°C");

                for fan in state.mapping.read().await.fans_for_sensor(&sensor_name) {
                    let controller_id = u8::try_from(fan.controller_id).map_err(|_| {
                        anyhow::anyhow!("Controller ID {} too large for u8", fan.controller_id)
                    })?;
                    let channel = u8::try_from(fan.channel)
                        .map_err(|_| anyhow::anyhow!("Channel {} too large for u8", fan.channel))?;

                    if let Err(e) = state
                        .controllers
                        .read()
                        .await
                        .update_channel(controller_id, channel, temp)
                        .await
                    {
                        log::error!("Failed to update controller: {e}");
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read temperature from sensor: {e}");
            }
        }
    }

    *state.sensor_data.write().await = temperatures.clone();

    if let Err(e) = event_bus.publish(Event::TemperatureChanged(temperatures)) {
        log::error!("Failed to publish temperature event: {e}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, FanTarget, MappingCfg, SensorCfg},
        controller::Controllers,
        sensors::TemperatureSensor,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
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

        let config_manager =
            crate::config::ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"));
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
        assert_eq!(task_manager.active_count(), 0);
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
            Controllers::init_from_cfg(&config).unwrap_or_else(|_| Controllers::empty());

        // Create AppState with our mock sensors
        let config_manager =
            crate::config::ConfigManager::new(config, std::path::PathBuf::from("/dev/null"));
        let state = Arc::new(crate::app_context::AppState {
            config_manager: Arc::new(config_manager),
            controllers: Arc::new(tokio::sync::RwLock::new(controllers)),
            sensors: Arc::new(tokio::sync::RwLock::new(sensors)),
            mapping: Arc::new(RwLock::new(crate::mappings::Mapping::load_mappings(&[]))),
            sensor_data: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            #[allow(dead_code)]
            color_mappings: Arc::new(RwLock::new(
                crate::mappings::ColorMapping::build_color_mapping(&[]),
            )),
        });

        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();
        let mut task_manager = TaskManager::new();

        let provider = MonitoringServiceProvider::new(state, event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Wait for temperature event
        let event = timeout(Duration::from_secs(3), receiver.recv()).await;
        assert!(event.is_ok());

        match event.unwrap().unwrap() {
            Event::TemperatureChanged(temperatures) => {
                assert_eq!(temperatures.len(), 2);
                assert!(temperatures.contains_key("cpu_temp"));
                assert!(temperatures.contains_key("gpu_temp"));
                assert_eq!(temperatures["cpu_temp"], 45.5);
                assert_eq!(temperatures["gpu_temp"], 62.3);
            }
            _ => panic!("Expected TemperatureChanged event"),
        }

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn monitoring_service_timing_configuration() {
        let config = Config {
            tick_seconds: 1, // 1 second intervals
            ..Default::default()
        };

        let _controllers =
            Controllers::init_from_cfg(&config).unwrap_or_else(|_| Controllers::empty());

        let config_manager =
            crate::config::ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"));
        let state = Arc::new(AppState::new(config_manager).await.unwrap());

        let event_bus = EventBus::new();
        let mut task_manager = TaskManager::new();

        let provider = MonitoringServiceProvider::new(state, event_bus);
        let result = provider.start(&mut task_manager).await;

        assert!(result.is_ok());

        // Service should start and run with custom timing
        sleep(Duration::from_millis(100)).await;
        assert!(task_manager.is_running("MonitoringService"));

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }
}
