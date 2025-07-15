//! System coordinator for managing service lifecycle and dependency injection.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tracing::{error, info, warn};

use crate::{
    app_context::AppState,
    config::ConfigManager,
    drivers::commands::{BatchCommand, ExecutionMode},
    event::{ConfigChangeType, Event, EventBus, PrepareConfigUpdateCommand, Response},
    providers::{
        AppStateProvider, AsyncProvider, BroadcastServiceProvider, ConfigWatcherServiceProvider,
        DBusServiceProvider, FanColorControlServiceProvider, MonitoringServiceProvider,
        ServiceProvider, UdevWatcherServiceProvider,
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
    transaction_counter: std::sync::atomic::AtomicU64,
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
            transaction_counter: std::sync::atomic::AtomicU64::new(1),
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
            .batch_update(BatchCommand::Init, ExecutionMode::Blocking)
            // .send_init()
            .await
            .context("Failed to initialize hardware controllers")?;

        self.register_service_providers(state.clone(), &config_manager)
            .await
            .context("Failed to register service providers")?;

        info!("SystemCoordinator initialization completed");
        Ok(())
    }

    /// Registers all service providers with prioritization.
    async fn register_service_providers(
        &mut self,
        state: Arc<AppState>,
        config: &ConfigManager,
    ) -> Result<()> {
        let mut providers: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(
                MonitoringServiceProvider::new(state.clone(), self.event_bus.clone(), config).await,
            ),
            Box::new(BroadcastServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
            Box::new(
                FanColorControlServiceProvider::new(state.clone(), self.event_bus.clone(), config)
                    .await,
            ),
            Box::new(ConfigWatcherServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
            Box::new(UdevWatcherServiceProvider::new(
                state.clone(),
                self.event_bus.clone(),
            )),
        ];

        match DBusServiceProvider::new(state.clone(), self.event_bus.clone()).await {
            Ok(provider) => {
                providers.push(Box::new(provider));
            }
            Err(e) => {
                warn!(
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
                    warn!(
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
            Ok(Event::DeviceConnected {
                vendor_id,
                product_id,
                serial_number,
            }) => {
                info!(
                    "Device connected: vendor_id={}, product_id={}, serial_number={:?}",
                    vendor_id, product_id, serial_number
                );
                self.handle_device_connected(vendor_id, product_id, serial_number)
                    .await
                    .context("Failed to handle device connected event")?;
            }
            Ok(Event::DeviceDisconnected {
                vendor_id,
                product_id,
                serial_number,
            }) => {
                info!(
                    "Device disconnected: vendor_id={}, product_id={}, serial_number={:?}",
                    vendor_id, product_id, serial_number
                );
                self.handle_device_disconnected(vendor_id, product_id, serial_number)
                    .await
                    .context("Failed to handle device disconnected event")?;
            }
            Ok(event) => {
                info!("Received event: {event:?}");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                bail!("Event bus channel closed unexpectedly");
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("Event bus lagged by {n} messages");
            }
        }
        Ok(())
    }

    async fn handle_device_connected(
        &self,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<String>,
    ) -> Result<()> {
        info!(
            "Handling device connected: vendor_id={}, product_id={}, serial_number={:?}",
            vendor_id, product_id, serial_number
        );

        if let Some(state) = &self.shared_state {
            state
                .controllers
                .write()
                .await
                .device_connected(vendor_id, product_id, serial_number)
                .await?
        } else {
            warn!("Cannot handle device connected event: system state not initialized");
        }

        Ok(())
    }

    async fn handle_device_disconnected(
        &self,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<String>,
    ) -> Result<()> {
        info!(
            "Handling device disconnected: vendor_id={}, product_id={}, serial_number={:?}",
            vendor_id, product_id, serial_number
        );

        if let Some(state) = &self.shared_state {
            state
                .controllers
                .write()
                .await
                .device_disconnected(vendor_id, product_id, serial_number)
                .await?
        } else {
            warn!("Cannot handle device disconnected event: system state not initialized");
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
                warn!(
                    "Hardware configuration changes detected in sections: {:?}",
                    changed_sections
                );
                warn!("These changes require daemon restart to take effect");
                warn!("Please restart the tt_riingd daemon to apply hardware changes");

                // Log user-friendly instructions
                info!("To restart the daemon, run:");
                info!("  sudo systemctl restart tt_riingd");
                info!("or stop and start the daemon manually");

                Ok(())
            }
        }
    }

    /// Generates a unique transaction ID for 2PC operations
    fn generate_transaction_id(&self) -> u64 {
        self.transaction_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Handles hot-reloadable configuration changes using 2PC through MessageBroker.
    ///
    /// This implements a Two-Phase Commit protocol:
    /// 1. Phase 1: Send PrepareConfigUpdate to all services via execute()
    /// 2. Phase 2: Send CommitConfigUpdate or RollbackConfigUpdate via notify()
    async fn handle_hot_reload(&self) -> Result<()> {
        info!("Starting 2PC hot configuration reload...");

        if let Some(state) = &self.shared_state {
            state
                .config_manager()
                .reload()
                .await
                .context("Failed to reload configuration")?;

            let transaction_id = self.generate_transaction_id();
            info!("Starting config transaction {}", transaction_id);

            info!("Phase 1: Preparing config update across all services...");

            match self
                .event_bus
                .execute(PrepareConfigUpdateCommand { transaction_id })
                .await
            {
                Ok(results) => {
                    let mut success_count = 0;
                    let mut failure_count = 0;
                    let mut failed_services = Vec::new();

                    for (service_type, response) in results {
                        match response {
                            Response::Success => {
                                success_count += 1;
                                info!("Service {:?}: Config preparation successful", service_type);
                            }
                            Response::Error(err) => {
                                failure_count += 1;
                                failed_services.push((service_type, err.clone()));
                                error!(
                                    "Service {:?}: Config preparation failed: {}",
                                    service_type, err
                                );
                            }
                            _ => {
                                failure_count += 1;
                                failed_services
                                    .push((service_type, "Unexpected response type".to_string()));
                                error!("Service {:?}: Unexpected response type", service_type);
                            }
                        }
                    }

                    if failure_count == 0 {
                        info!(
                            "Phase 2: All services prepared successfully ({} services) - committing transaction {}",
                            success_count, transaction_id
                        );

                        self.event_bus
                            .notify(crate::event::Event::CommitConfigUpdate { transaction_id })?;
                        info!("Hot configuration reload completed successfully!");
                    } else {
                        warn!(
                            "Phase 2: {} service(s) failed preparation - rolling back transaction {}",
                            failure_count, transaction_id
                        );

                        for (service, error) in &failed_services {
                            warn!("  - Service {:?}: {}", service, error);
                        }

                        self.event_bus
                            .notify(crate::event::Event::RollbackConfigUpdate { transaction_id })?;

                        return Err(anyhow::anyhow!(
                            "Config reload failed: {} services could not prepare the new configuration",
                            failure_count
                        ));
                    }
                }
                Err(e) => {
                    error!("Phase 1: Failed to execute PrepareConfigUpdate: {}", e);

                    self.event_bus
                        .notify(crate::event::Event::RollbackConfigUpdate { transaction_id })?;

                    return Err(e).context("Failed to prepare config update across services");
                }
            }
        } else {
            warn!("Cannot reload config: system state not initialized");
            return Err(anyhow::anyhow!("System state not initialized"));
        }

        Ok(())
    }

    /// Performs graceful shutdown of all components.
    async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating graceful shutdown...");

        if let Err(e) = self.task_manager.shutdown_all().await {
            error!("Error during task shutdown: {}", e);
        }

        if let Some(state) = &self.shared_state {
            state.controllers.write().await.shutdown().await;
        } else {
            warn!("Cannot shutdown: system state not initialized");
        }

        info!("Shutdown complete");
        Ok(())
    }
}
