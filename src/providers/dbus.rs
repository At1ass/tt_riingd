//! D-Bus service provider for dependency injection.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;
use zbus::Connection;

use crate::{
    core::{app_context::AppState, event::MessageBroker, task_manager::TaskManager},
    interface::dbus_interface::DBusInterface,
    providers::traits::ServiceProvider,
};

/// D-Bus service provider for external system integration.
///
/// Provides a critical service that exposes daemon functionality through
/// D-Bus interface, enabling external applications to interact with the
/// fan control daemon. This service runs on the session bus and handles
/// method calls and property access.
///
/// # Priority and Criticality
///
/// - **Priority**: 8 (high)
/// - **Critical**: Yes (important for system integration)
///
/// # Features
///
/// - D-Bus method call handling
/// - Property exposure for system monitoring
/// - Event signal broadcasting
/// - Session bus integration
/// - Automatic service name registration
///
/// # Interface
///
/// Exposes interface at:
/// - **Service Name**: `io.github.tt_riingd`
/// - **Object Path**: `/io/github/tt_riingd`
///
/// # Requirements
///
/// Requires a running D-Bus session bus. Creation will fail if D-Bus is
/// not available, which is handled gracefully by the system coordinator.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::DBusServiceProvider;
/// use tt_riingd::event::MessageBroker;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = MessageBroker::new();
/// // Note: This may fail if D-Bus session is not available
/// let provider = DBusServiceProvider::new(state, event_bus).await?;
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub struct DBusServiceProvider {
    state: Arc<AppState>,
    event_bus: MessageBroker,
    connection: Connection,
}

impl DBusServiceProvider {
    /// Creates a new D-Bus service provider with session bus connection.
    pub async fn new(state: Arc<AppState>, event_bus: MessageBroker) -> Result<Self> {
        let connection = Connection::session().await?;
        Ok(Self {
            state,
            event_bus,
            connection,
        })
    }
}

#[async_trait]
impl ServiceProvider for DBusServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let connection = self.connection.clone();

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_dbus_service(state, event_bus, connection, cancel_token).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "DBusService"
    }

    fn priority(&self) -> i32 {
        8
    }

    fn is_critical(&self) -> bool {
        true
    }
}

/// D-Bus service for exposing daemon functionality to external applications.
///
/// Runs the D-Bus interface on the session bus and handles incoming requests
/// until cancellation is requested.
async fn run_dbus_service(
    state: Arc<AppState>,
    event_bus: MessageBroker,
    connection: Connection,
    cancel_token: CancellationToken,
) -> Result<()> {
    let interface = DBusInterface::new(state, env!("CARGO_PKG_VERSION").to_string(), event_bus);
    connection
        .object_server()
        .at("/io/github/tt_riingd", interface)
        .await?;

    connection.request_name("io.github.tt_riingd").await?;

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("D-Bus service cancelled");
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                // Keep connection alive
            }
        }
    }

    Ok(())
}
