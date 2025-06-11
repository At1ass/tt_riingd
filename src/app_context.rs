//! Application state and global context management.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use tokio::sync::RwLock;

use crate::{
    config::{Config, ConfigManager},
    controller,
    mappings::{ColorMapping, Mapping},
    sensors::TemperatureSensor,
    temperature_sensors::lm_sensor,
};

/// Shared application state containing all runtime data.
///
/// This structure holds all the shared state needed by various services,
/// including hardware controllers, sensors, mappings, and runtime data.
/// All fields are wrapped in appropriate synchronization primitives for
/// safe concurrent access.
pub struct AppState {
    /// Configuration manager for centralized config handling
    pub config_manager: Arc<ConfigManager>,
    /// Hardware controllers for fan management
    pub controllers: Arc<RwLock<controller::Controllers>>,
    /// Temperature sensors for monitoring
    pub sensors: Arc<RwLock<Vec<Box<dyn TemperatureSensor>>>>,
    /// Sensor-to-fan mappings
    pub mapping: Arc<Mapping>,
    /// Color-to-fan mappings
    #[allow(dead_code)] // Used in future RGB color control features
    pub color_mappings: Arc<ColorMapping>,
    /// Runtime sensor data cache
    pub sensor_data: Arc<RwLock<HashMap<String, f32>>>,
}

/// Wrapper for lm-sensors library instance.
///
/// This wrapper is needed to implement Send + Sync for the lm-sensors
/// library which doesn't implement these traits by default.
pub struct LMSensorsRef(pub lm_sensors::LMSensors);

// SAFETY: lm-sensors library (>= 3.6) uses internal global mutex for all operations.
// The library is thread-safe but doesn't implement Send/Sync markers.
unsafe impl Send for LMSensorsRef {}
unsafe impl Sync for LMSensorsRef {}

/// Global lm-sensors instance.
///
/// Initialized once at startup and shared across all temperature sensor instances.
/// Uses LazyLock for thread-safe lazy initialization.
/// Returns None if lm-sensors is not available on the system.
pub static LMSENSORS: LazyLock<Option<LMSensorsRef>> =
    LazyLock::new(|| match lm_sensors::Initializer::default().initialize() {
        Ok(sensors) => {
            log::info!("lm-sensors initialized successfully");
            Some(LMSensorsRef(sensors))
        }
        Err(e) => {
            log::warn!(
                "lm-sensors not available: {}. Temperature monitoring will be limited.",
                e
            );
            None
        }
    });

impl AppState {
    /// Creates a new AppState from the given configuration manager.
    ///
    /// This performs synchronous initialization of hardware components.
    /// For async initialization, use the AppStateProvider instead.
    pub async fn new(config_manager: ConfigManager) -> anyhow::Result<Self> {
        let config = config_manager.clone_config().await;

        Ok(Self {
            controllers: Arc::new(RwLock::new(
                controller::Controllers::init_from_cfg(&config)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize controllers: {}", e))?,
            )),
            sensors: Arc::new(RwLock::new(match LMSENSORS.as_ref() {
                Some(lms) => lm_sensor::LmSensorSource::discover(&lms.0, &config.sensors),
                None => {
                    log::warn!(
                        "lm-sensors not available, no temperature sensors will be discovered"
                    );
                    Vec::new()
                }
            })),
            mapping: Arc::new(Mapping::load_mappings(&config.mappings)),
            color_mappings: Arc::new(ColorMapping::build_color_mapping(&config.color_mappings)),
            sensor_data: Arc::new(RwLock::new(HashMap::new())),
            config_manager: Arc::new(config_manager),
        })
    }

    /// Gets a read-only reference to the current configuration.
    pub async fn config(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config_manager.get().await
    }

    /// Gets the configuration manager.
    pub fn config_manager(&self) -> &Arc<ConfigManager> {
        &self.config_manager
    }


}
