//! Application state and global context management.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    config::{Config, ConfigManager},
    drivers::controller_manager,
    mappings::{ColorMapping, CurveMapping, Mapping},
    temperature_sensors::sensor_manager,
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
    pub controllers: Arc<RwLock<controller_manager::ControllerManager>>,
    /// Temperature sensors for monitoring
    pub sensors: Arc<RwLock<sensor_manager::SensorManager>>,
    /// Sensor-to-fan mappings
    pub mapping: Arc<RwLock<Mapping>>,
    /// Color-to-fan mappings
    #[allow(dead_code)] // Will be used for future RGB color control features
    pub color_mappings: Arc<RwLock<ColorMapping>>,
    /// Active curve assignments for each fan - maps (controller_id, channel) to curve name
    pub active_curves: Arc<RwLock<CurveMapping>>,
    /// Runtime sensor data cache
    pub sensor_data: Arc<RwLock<HashMap<String, f32>>>,
}

impl AppState {
    /// Creates a new AppState from the given configuration manager.
    ///
    /// This performs synchronous initialization of hardware components.
    /// For async initialization, use the AppStateProvider instead.
    pub async fn new(config_manager: ConfigManager) -> anyhow::Result<Self> {
        let config = config_manager.clone_config().await;

        Ok(Self {
            controllers: Arc::new(RwLock::new(
                controller_manager::ControllerManager::init_from_cfg(&config)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize controllers: {}", e))?,
            )),
            sensors: Arc::new(RwLock::new(
                sensor_manager::SensorManager::init_from_cfg(&config)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize sensors: {}", e))?,
            )),
            mapping: Arc::new(RwLock::new(Mapping::load_mappings(&config.mappings))),
            color_mappings: Arc::new(RwLock::new(ColorMapping::build_color_mapping(
                &config.color_mappings,
            ))),
            active_curves: Arc::new(RwLock::new(CurveMapping::load_mappings(
                &config.active_curve_mappings,
            ))),
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

    pub async fn update_mappings(&self, config: &Config) -> anyhow::Result<()> {
        let new_mapping = Mapping::load_mappings(&config.mappings);
        let new_clr_mappings = ColorMapping::build_color_mapping(&config.color_mappings);
        let new_active_curves = CurveMapping::load_mappings(&config.active_curve_mappings);

        let mut mapping = self.mapping.write().await;
        *mapping = new_mapping;

        let mut color_mappings = self.color_mappings.write().await;
        *color_mappings = new_clr_mappings;

        let mut active_curves = self.active_curves.write().await;
        *active_curves = new_active_curves;

        Ok(())
    }
}
