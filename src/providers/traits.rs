use anyhow::Result;
use async_trait::async_trait;

use crate::task_manager::TaskManager;

/// Base trait for providers that can create components asynchronously.
///
/// Enables dependency injection pattern with async initialization support.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::providers::traits::AsyncProvider;
/// use std::sync::Arc;
///
/// struct ConfigProvider;
///
/// #[async_trait::async_trait]
/// impl AsyncProvider<String> for ConfigProvider {
///     async fn provide(&self) -> anyhow::Result<String> {
///         Ok("config data".to_string())
///     }
/// }
/// ```
#[async_trait]
pub trait AsyncProvider<T> {
    async fn provide(&self) -> Result<T>;
}

/// Trait for services that can be started through TaskManager.
///
/// Provides service lifecycle management with prioritization and
/// criticality classification for graceful degradation.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::providers::traits::ServiceProvider;
/// use tt_riingd::task_manager::TaskManager;
/// use anyhow::Result;
///
/// struct ExampleService;
///
/// #[async_trait::async_trait]
/// impl ServiceProvider for ExampleService {
///     async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
///         task_manager.spawn_task("example".to_string(), |_token| async {
///             // Service implementation
///             Ok(())
///         }).await
///     }
///     
///     fn name(&self) -> &'static str { "ExampleService" }
///     fn priority(&self) -> i32 { 5 }
///     fn is_critical(&self) -> bool { false }
/// }
/// ```
#[async_trait]
pub trait ServiceProvider: Send + Sync {
    /// Starts the service in TaskManager.
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()>;

    /// Returns service name for logging and management.
    fn name(&self) -> &'static str;

    /// Returns startup priority (higher numbers start first).
    fn priority(&self) -> i32 {
        0
    }

    /// Indicates if service is critical for system operation.
    fn is_critical(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "tests/traits_test.rs"]
mod traits_test;
