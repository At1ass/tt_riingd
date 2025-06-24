//! Application entry point and builder pattern implementation.

use crate::{config::ConfigManager, coordinator::SystemCoordinator};
use anyhow::Result;

/// Main application structure that orchestrates all daemon components.
///
/// Manages the complete lifecycle from initialization to shutdown,
/// coordinating all services through the SystemCoordinator.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::application::Application;
/// use tt_riingd::config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config_manager = config::ConfigManager::load(None).await?;
/// let mut app = Application::builder()
///     .with_config_manager(config_manager)
///     .build()
///     .await?;
///
/// app.run().await?;
/// # Ok(())
/// # }
/// ```
pub struct Application {
    pub coordinator: SystemCoordinator,
    config_manager: ConfigManager,
}

impl Application {
    /// Creates a new ApplicationBuilder for constructing Application instances.
    pub fn builder() -> ApplicationBuilder {
        ApplicationBuilder::new()
    }

    /// Runs the complete daemon lifecycle: initialize, start services, and run main loop.
    pub async fn run(&mut self) -> Result<()> {
        self.coordinator
            .initialize(self.config_manager.clone())
            .await?;

        self.coordinator.start_all_services().await?;

        self.coordinator.run_main_loop().await?;

        Ok(())
    }
}

/// Builder pattern for creating Application instances.
///
/// Provides a fluent interface for configuring the application before startup.
pub struct ApplicationBuilder {
    config_manager: Option<ConfigManager>,
}

impl ApplicationBuilder {
    fn new() -> Self {
        Self {
            config_manager: None,
        }
    }

    /// Sets the configuration manager for the application.
    pub fn with_config_manager(mut self, config_manager: ConfigManager) -> Self {
        self.config_manager = Some(config_manager);
        self
    }

    /// Builds the Application instance with the provided configuration.
    pub async fn build(self) -> Result<Application> {
        let config_manager = self
            .config_manager
            .ok_or_else(|| anyhow::anyhow!("Configuration manager is required"))?;
        let coordinator = SystemCoordinator::new();

        Ok(Application {
            coordinator,
            config_manager,
        })
    }
}
