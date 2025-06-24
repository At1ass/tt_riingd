//! Mock implementations for testing.
//!
//! This module provides mock implementations of various system components
//! to enable testing without hardware dependencies. Following Rust testing
//! best practices and using mockall for clean, maintainable mocks.

use anyhow::Result;
use async_trait::async_trait;
use mockall::{automock, predicate::*};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use tt_riingd::{
    config::{Config, ControllerCfg},
    drivers::fan_controller::FanController,
    fan_curve::Point,
};

/// Mock implementation of FanController for testing hardware interactions.
///
/// This mock allows testing fan control logic without real USB devices.
/// Following the best practice of mocking external dependencies.
#[automock]
#[async_trait]
#[allow(dead_code)]
pub trait MockableFanController: Send + Sync {
    async fn send_init(&self) -> Result<()>;
    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()>;
    async fn update_channel_color(&self, channel: u8, red: u8, green: u8, blue: u8) -> Result<()>;
    async fn firmware_version(&self) -> Result<(u8, u8, u8)>;
}

/// Mock implementation of TemperatureSensor for testing monitoring logic.
///
/// Allows controlled temperature readings for testing curve calculations
/// and sensor mapping logic.
#[automock]
#[async_trait]
#[allow(dead_code)]
pub trait MockableTemperatureSensor: Send + Sync {
    async fn read_temperature(&self) -> Result<f32>;
    fn sensor_key(&self) -> String;
}

/// Real FanController wrapper to make it mockable.
///
/// This adapter pattern allows us to mock the FanController trait
/// while maintaining compatibility with existing code.
#[allow(dead_code)]
pub struct FanControllerAdapter<T: FanController> {
    inner: T,
}

#[allow(dead_code)]
impl<T: FanController> FanControllerAdapter<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T: FanController> MockableFanController for FanControllerAdapter<T> {
    async fn send_init(&self) -> Result<()> {
        self.inner.send_init().await
    }

    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> {
        self.inner.update_channel(channel, temp, speed).await
    }

    async fn update_channel_color(&self, channel: u8, red: u8, green: u8, blue: u8) -> Result<()> {
        self.inner
            .update_channel_color(channel, red, green, blue)
            .await
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        self.inner.firmware_version().await
    }
}

/// Mock configuration manager for testing configuration loading and reloading.
///
/// Provides controlled configuration scenarios without filesystem dependencies.
#[derive(Clone)]
#[allow(dead_code)]
pub struct MockConfigManager {
    config: Arc<RwLock<Config>>,
    fail_reload: bool,
}

#[allow(dead_code)]
impl MockConfigManager {
    /// Creates a new mock config manager with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            fail_reload: false,
        }
    }

    /// Creates a mock that will fail on reload attempts.
    pub fn new_failing() -> Self {
        Self {
            config: Arc::new(RwLock::new(Config::default())),
            fail_reload: true,
        }
    }

    /// Updates the configuration (simulating a file change).
    pub async fn update_config(&self, new_config: Config) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    /// Simulates a configuration reload.
    pub async fn reload(&self) -> Result<()> {
        if self.fail_reload {
            return Err(anyhow::anyhow!("Mock reload failure"));
        }
        Ok(())
    }

    /// Gets the current configuration.
    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
    }
}

/// Mock AppState for testing without hardware initialization.
///
/// This provides a complete mock of the application state that can be used
/// in integration tests without requiring real hardware or filesystem access.
#[derive(Clone)]
#[allow(dead_code)]
pub struct MockAppState {
    pub config_manager: MockConfigManager,
    pub sensor_data: Arc<RwLock<HashMap<String, f32>>>,
}

#[allow(dead_code)]
impl MockAppState {
    /// Creates a new mock app state with the given configuration.
    pub async fn new(config: Config) -> Self {
        Self {
            config_manager: MockConfigManager::new(config),
            sensor_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a mock app state that will fail on configuration reload.
    pub async fn new_failing() -> Self {
        Self {
            config_manager: MockConfigManager::new_failing(),
            sensor_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Updates sensor data for testing.
    pub async fn set_sensor_data(&self, sensor: &str, temperature: f32) {
        let mut data = self.sensor_data.write().await;
        data.insert(sensor.to_string(), temperature);
    }

    /// Gets sensor data for testing.
    pub async fn get_sensor_data(&self, sensor: &str) -> Option<f32> {
        let data = self.sensor_data.read().await;
        data.get(sensor).copied()
    }
}

/// Test utilities for creating common test configurations.
#[allow(dead_code)]
pub mod test_utils {
    use super::*;
    use tt_riingd::config::*;

    /// Creates a minimal valid configuration for testing.
    #[allow(dead_code)]
    pub fn minimal_config() -> Config {
        Config {
            version: 1,
            tick_seconds: 2,
            enable_broadcast: false,
            broadcast_interval: 30,
            controllers: vec![],
            curves: vec![],
            sensors: vec![],
            mappings: vec![],
            active_curve_mappings: vec![],
            effects: vec![],
            effect_mappings: vec![],
        }
    }

    /// Creates a configuration with a single controller for testing.
    #[allow(dead_code)]
    pub fn single_controller_config() -> Config {
        Config {
            version: 1,
            tick_seconds: 2,
            enable_broadcast: false,
            broadcast_interval: 30,
            controllers: vec![ControllerCfg::RiingQuad {
                id: "test_controller".to_string(),
                usb: UsbSelector {
                    vid: 0x264a,
                    pid: 0x2330,
                    serial: None,
                },
                fans: vec![FanCfg {
                    idx: 1,
                    name: "Test Fan".to_string(),
                }],
            }],
            curves: vec![CurveCfg::Constant {
                id: "test_curve".to_string(),
                speed: 50,
            }],
            sensors: vec![SensorCfg::LmSensors {
                id: "test_sensor".to_string(),
                chip: "test_chip".to_string(),
                feature: "test_feature".to_string(),
            }],
            mappings: vec![MappingCfg {
                sensor: "test_sensor".to_string(),
                targets: vec![FanTarget {
                    controller: 1,
                    fan_idx: 1,
                }],
            }],
            active_curve_mappings: vec![CurveMappingCfg {
                curve: "test_curve".to_string(),
                targets: vec![FanTarget {
                    controller: 1,
                    fan_idx: 1,
                }],
            }],
            effects: vec![],
            effect_mappings: vec![],
        }
    }

    /// Creates a complex configuration with multiple controllers and curves.
    #[allow(dead_code)]
    pub fn complex_config() -> Config {
        Config {
            version: 1,
            tick_seconds: 3,
            enable_broadcast: true,
            broadcast_interval: 15,
            controllers: vec![ControllerCfg::RiingQuad {
                id: "main_controller".to_string(),
                usb: UsbSelector {
                    vid: 0x264a,
                    pid: 0x2330,
                    serial: None,
                },
                fans: vec![
                    FanCfg {
                        idx: 1,
                        name: "CPU Intake".to_string(),
                    },
                    FanCfg {
                        idx: 2,
                        name: "CPU Exhaust".to_string(),
                    },
                    FanCfg {
                        idx: 3,
                        name: "Case Intake".to_string(),
                    },
                ],
            }],
            curves: vec![
                CurveCfg::Constant {
                    id: "silent".to_string(),
                    speed: 30,
                },
                CurveCfg::StepCurve {
                    id: "performance".to_string(),
                    tmps: vec![25.0, 45.0, 65.0, 80.0],
                    spds: vec![20, 40, 70, 95],
                },
                CurveCfg::Bezier {
                    id: "smooth".to_string(),
                    points: vec![
                        Point { x: 30.0, y: 20.0 },
                        Point { x: 45.0, y: 35.0 },
                        Point { x: 65.0, y: 70.0 },
                        Point { x: 80.0, y: 90.0 },
                    ],
                },
            ],
            sensors: vec![
                SensorCfg::LmSensors {
                    id: "cpu_temp".to_string(),
                    chip: "k10temp-pci-00c3".to_string(),
                    feature: "Tctl".to_string(),
                },
                SensorCfg::LmSensors {
                    id: "gpu_temp".to_string(),
                    chip: "amdgpu-pci-0300".to_string(),
                    feature: "junction".to_string(),
                },
            ],
            mappings: vec![
                MappingCfg {
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
                },
                MappingCfg {
                    sensor: "gpu_temp".to_string(),
                    targets: vec![FanTarget {
                        controller: 1,
                        fan_idx: 3,
                    }],
                },
            ],
            active_curve_mappings: vec![
                CurveMappingCfg {
                    curve: "performance".to_string(),
                    targets: vec![FanTarget {
                        controller: 1,
                        fan_idx: 1,
                    }],
                },
                CurveMappingCfg {
                    curve: "smooth".to_string(),
                    targets: vec![FanTarget {
                        controller: 1,
                        fan_idx: 2,
                    }],
                },
                CurveMappingCfg {
                    curve: "silent".to_string(),
                    targets: vec![FanTarget {
                        controller: 1,
                        fan_idx: 3,
                    }],
                },
            ],
            effects: vec![
                EffectCfg::ConstantColor {
                    id: "cool_blue".to_string(),
                    rgb: [0, 100, 255],
                },
                EffectCfg::ConstantColor {
                    id: "warm_orange".to_string(),
                    rgb: [255, 150, 0],
                },
                EffectCfg::ConstantColor {
                    id: "danger_red".to_string(),
                    rgb: [255, 0, 0],
                },
            ],
            effect_mappings: vec![
                EffectMappingCfg {
                    effect: "cool_blue".to_string(),
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
                },
                EffectMappingCfg {
                    effect: "warm_orange".to_string(),
                    targets: vec![FanTarget {
                        controller: 1,
                        fan_idx: 3,
                    }],
                },
            ],
        }
    }
}

/// Helper macros for creating mock expectations.
///
/// These macros follow the data-driven testing pattern recommended
/// in rust-analyzer architecture documentation.
#[macro_export]
macro_rules! mock_fan_controller {
    ($name:ident) => {
        let mut $name = MockMockableFanController::new();
    };
}

#[macro_export]
macro_rules! expect_init_ok {
    ($mock:expr) => {
        $mock.expect_send_init().times(1).returning(|| Ok(()));
    };
}

#[macro_export]
macro_rules! expect_update_channel {
    ($mock:expr, $channel:expr, $temp:expr, $speed:expr) => {
        $mock
            .expect_update_channel()
            .with(eq($channel), eq($temp), eq($speed))
            .times(1)
            .returning(|_, _, _| Ok(()));
    };
}

#[macro_export]
macro_rules! expect_firmware_version {
    ($mock:expr, $version:expr) => {
        $mock
            .expect_firmware_version()
            .times(1)
            .returning(move || Ok($version));
    };
}
