//! Application state provider for dependency injection.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::{app_context::AppState, config::ConfigManager, providers::traits::AsyncProvider};

/// Provider for creating and initializing application state.
///
/// Handles async initialization of hardware controllers, sensors,
/// and other components that require blocking operations.
pub struct AppStateProvider {
    config_manager: ConfigManager,
}

impl AppStateProvider {
    /// Creates a new AppStateProvider with the given configuration manager.
    pub const fn new(config_manager: ConfigManager) -> Self {
        Self { config_manager }
    }

    /// Returns a reference to the configuration manager.
    #[allow(dead_code)]
    pub const fn config_manager(&self) -> &ConfigManager {
        &self.config_manager
    }
}

#[async_trait]
impl AsyncProvider<Arc<AppState>> for AppStateProvider {
    async fn provide(&self) -> Result<Arc<AppState>> {
        let app_state = AppState::new(self.config_manager.clone()).await?;
        Ok(Arc::new(app_state))
    }
}
