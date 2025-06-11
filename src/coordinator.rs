//! System coordinator for managing service lifecycle and dependency injection.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use log::info;

use crate::{
    app_context::AppState,
    config::ConfigManager,
    event::{ConfigChangeType, Event, EventBus},
    providers::{
        AppStateProvider, AsyncProvider, BroadcastServiceProvider, ConfigWatcherServiceProvider,
        DBusServiceProvider, FanColorControlServiceProvider, MonitoringServiceProvider,
        ServiceProvider,
    },
    task_manager::TaskManager,
};

/// Enhanced SystemCoordinator with Dependency Injection pattern.
///
/// Manages the complete lifecycle of all services using a provider-based
/// architecture for loose coupling and testability.
///
/// # Features
/// - Service prioritization (critical vs non-critical)
/// - Graceful degradation on service failures
/// - Event-driven communication between services
/// - Proper async initialization and shutdown
pub struct SystemCoordinator {
    task_manager: TaskManager,
    event_bus: EventBus,
    shared_state: Option<Arc<AppState>>,
    service_providers: Vec<Box<dyn ServiceProvider>>,
}

impl Default for SystemCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemCoordinator {
    /// Creates a new coordinator with the given configuration.
    pub fn new() -> Self {
        let event_bus = EventBus::new();

        Self {
            task_manager: TaskManager::new(),
            event_bus,
            shared_state: None,
            service_providers: Vec::new(),
        }
    }

    /// Asynchronously initializes all components.
    ///
    /// This fixes blocking initialization by moving hardware operations
    /// to async context with proper error handling.
    pub async fn initialize(&mut self, config_manager: ConfigManager) -> Result<()> {
        info!("Initializing SystemCoordinator...");

        let app_state_provider = AppStateProvider::new(config_manager.clone());
        self.shared_state = Some(
            app_state_provider
                .provide()
                .await
                .context("Failed to initialize application state")?,
        );

        let state = self
            .shared_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("System not properly initialized"))?
            .clone();

        state
            .controllers
            .read()
            .await
            .send_init()
            .await
            .context("Failed to initialize hardware controllers")?;

        self.register_service_providers(state.clone())
            .await
            .context("Failed to register service providers")?;

        info!("SystemCoordinator initialization completed");
        Ok(())
    }

    /// Registers all service providers with prioritization.
    async fn register_service_providers(&mut self, state: Arc<AppState>) -> Result<()> {
        let mut providers: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(MonitoringServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
            Box::new(BroadcastServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
            Box::new(FanColorControlServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
            Box::new(ConfigWatcherServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
        ];

        match DBusServiceProvider::new(state.clone(), self.event_bus.clone()).await {
            Ok(provider) => {
                providers.push(Box::new(provider));
            }
            Err(e) => {
                log::warn!(
                    "Failed to create D-Bus service provider: {}, skipping D-Bus service",
                    e
                );
            }
        }

        providers.sort_by_key(|b| std::cmp::Reverse(b.priority()));
        self.service_providers = providers;

        info!(
            "Registered {} service providers in priority order",
            self.service_providers.len()
        );

        Ok(())
    }

    /// Starts all registered services in priority order.
    ///
    /// Critical services must start successfully, while non-critical services
    /// can fail without stopping the system.
    pub async fn start_all_services(&mut self) -> Result<()> {
        info!(
            "Starting {} services in priority order...",
            self.service_providers.len()
        );

        for provider in &self.service_providers {
            let is_critical = provider.is_critical();

            match provider.start(&mut self.task_manager).await {
                Ok(()) => {
                    info!(
                        "Service '{}' started successfully (priority: {}, critical: {})",
                        provider.name(),
                        provider.priority(),
                        is_critical
                    );
                }
                Err(e) if is_critical => {
                    return Err(e).with_context(|| {
                        format!("Critical service '{}' failed to start", provider.name())
                    });
                }
                Err(e) => {
                    log::warn!(
                        "Non-critical service '{}' failed to start: {}",
                        provider.name(),
                        e
                    );
                }
            }
        }

        info!("All critical services started successfully");
        Ok(())
    }

    /// Main event loop with enhanced error handling.
    pub async fn run_main_loop(&mut self) -> Result<()> {
        let mut event_rx = self.event_bus.subscribe();
        info!("Starting main event loop");

        loop {
            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    match result {
                        Ok(()) => {
                            info!("Received Ctrl+C, initiating graceful shutdown...");
                            self.shutdown().await
                                .context("Failed to shutdown gracefully after Ctrl+C")?;
                            break;
                        }
                        Err(e) => {
                            bail!("Failed to listen for shutdown signal: {}", e);
                        }
                    }
                }

                event = event_rx.recv() => {
                    self.handle_event(event).await?;
                }
            }
        }

        info!("Main event loop terminated");
        Ok(())
    }

    /// Handles application events.
    async fn handle_event(
        &mut self,
        event_result: Result<Event, tokio::sync::broadcast::error::RecvError>,
    ) -> Result<()> {
        match event_result {
            Ok(Event::ConfigChangeDetected(change_type)) => {
                info!("Processing ConfigChangeDetected event");
                self.handle_config_change(change_type)
                    .await
                    .context("Failed to handle config change")?;
            }
            Ok(Event::SystemShutdown) => {
                info!("Processing SystemShutdown event");
                self.shutdown()
                    .await
                    .context("Failed to shutdown gracefully after SystemShutdown event")?;
                return Err(anyhow::anyhow!("System shutdown requested"));
            }
            Ok(event) => {
                info!("Received event: {event:?}");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                bail!("Event bus channel closed unexpectedly");
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("Event bus lagged by {n} messages");
            }
        }
        Ok(())
    }

    /// Handles configuration change based on type.
    async fn handle_config_change(&self, change_type: ConfigChangeType) -> Result<()> {
        match change_type {
            ConfigChangeType::HotReload => {
                info!("Applying hot-reloadable configuration changes...");
                self.handle_hot_reload().await
            }
            ConfigChangeType::ColdRestart { changed_sections } => {
                log::warn!("Hardware configuration changes detected in sections: {:?}", changed_sections);
                log::warn!("These changes require daemon restart to take effect");
                log::warn!("Please restart the tt_riingd daemon to apply hardware changes");
                
                // Log user-friendly instructions
                log::info!("To restart the daemon, run:");
                log::info!("  sudo systemctl restart tt_riingd");
                log::info!("or stop and start the daemon manually");
                
                Ok(())
            }
        }
    }

    /// Handles hot-reloadable configuration changes.
    async fn handle_hot_reload(&self) -> Result<()> {
        info!("Applying hot-reloadable configuration changes...");

        if let Some(state) = &self.shared_state {
            // Reload only the hot-reloadable parts of configuration
            state
                .config_manager()
                .reload()
                .await
                .context("Failed to reload configuration")?;
            
            // Update mappings and other hot-reloadable components
            let _new_config = state.config_manager().clone_config().await;
            
            // Note: Controllers and sensors are NOT reinitialized for hot reload
            // Only mappings, curves, and colors are updated
            log::info!("Updated configuration for curves, mappings, and colors");
            log::info!("Hot configuration reload completed successfully");
        } else {
            log::warn!("Cannot reload config: system state not initialized");
        }

        Ok(())
    }



    /// Performs graceful shutdown of all components.
    async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating graceful shutdown...");

        if let Err(e) = self.task_manager.shutdown_all().await {
            log::error!("Error during task shutdown: {}", e);
        }

        info!("Shutdown complete");
        Ok(())
    }

    /// Returns a reference to the EventBus for testing purposes.
    #[allow(dead_code)]
    pub const fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    #[allow(dead_code)]
    pub fn running_services(&self) -> Vec<&'static str> {
        self.service_providers.iter().map(|p| p.name()).collect()
    }
}
