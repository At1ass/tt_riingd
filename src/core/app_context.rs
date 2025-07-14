//! Application state and global context management.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{
    config::{Config, ConfigManager},
    drivers::controller_manager,
    temperature_sensors::sensor_manager,
};

/// Shared application state containing all runtime data.
///
/// This structure holds all the shared state needed by various services,
/// including hardware controllers, sensors, mappings, and runtime data.
/// All fields are wrapped in appropriate synchronization primitives for
/// safe concurrent access.
#[derive(Clone)]
pub struct AppState {
    /// Configuration manager for centralized config handling
    pub config_manager: Arc<ConfigManager>,
    /// Hardware controllers for fan management
    pub controllers: Arc<RwLock<controller_manager::ControllerManager>>,
    /// Temperature sensors for monitoring
    pub sensors: Arc<RwLock<sensor_manager::SensorManager>>,
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
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to initialize controllers: {}", e))?,
            )),
            sensors: Arc::new(RwLock::new(
                sensor_manager::SensorManager::init_from_cfg(&config)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize sensors: {}", e))?,
            )),
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
