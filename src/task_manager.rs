//! Task management for async service lifecycle.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use log::{error, info, warn};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Manages async tasks with proper lifecycle and error handling.
///
/// Provides centralized management of background tasks with graceful shutdown
/// capabilities and error propagation.
pub struct TaskManager {
    tasks: HashMap<String, TaskInfo>,
    pub global_token: CancellationToken,
}

impl TaskManager {
    /// Creates a new TaskManager.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            global_token: CancellationToken::new(),
        }
    }

    /// Spawns and registers a task with the given name.
    ///
    /// The task will be tracked and can be shut down gracefully.
    pub async fn spawn_task<F, Fut>(&mut self, name: String, task_fn: F) -> Result<()>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let task_token = self.global_token.child_token();
        let task_token_clone = task_token.clone();
        let task_name = name.clone();

        let handle = tokio::spawn(async move {
            info!("Starting task: {}", task_name);
            match task_fn(task_token_clone).await {
                Ok(()) => {
                    info!("Task '{}' completed successfully", task_name);
                    Ok(())
                }
                Err(e) => {
                    error!("Task '{}' failed: {}", task_name, e);
                    Err(e)
                }
            }
        });

        self.tasks.insert(
            name.clone(),
            TaskInfo {
                handle,
                cancel_token: task_token,
            },
        );

        info!("Task '{}' spawned", name);
        Ok(())
    }

    /// Shuts down all registered tasks gracefully.
    ///
    /// Waits for all tasks to complete and collects any errors.
    /// Returns the first error encountered, if any.
    pub async fn shutdown_all(&mut self) -> Result<()> {
        info!("Stopping all {} tasks", self.tasks.len());

        self.global_token.cancel();

        let mut first_error = None;
        let handles: Vec<_> = self.tasks.drain().map(|(_, info)| info.handle).collect();

        for handle in handles {
            match tokio::time::timeout(Duration::from_secs(10), handle).await {
                Ok(Ok(Ok(()))) => {
                    // Task completed successfully
                }
                Ok(Ok(Err(e))) => {
                    warn!("Task failed during shutdown: {}", e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                Ok(Err(e)) => {
                    let error = anyhow::anyhow!("Task panicked: {}", e);
                    error!("{}", error);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(_) => {
                    let error = anyhow::anyhow!("Task shutdown timeout exceeded");
                    error!("{}", error);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }

        if let Some(error) = first_error {
            Err(error).context("One or more tasks failed during shutdown")
        } else {
            info!("All tasks stopped");
            Ok(())
        }
    }

    /// Returns the count of active tasks.
    ///
    /// Used only for testing purposes.
    #[cfg(test)]
    pub fn active_count(&self) -> usize {
        self.tasks.len()
    }

    /// Checks if a task with the given name is currently running.
    ///
    /// Used only for testing purposes.
    #[cfg(test)]
    pub fn is_running(&self, name: &str) -> bool {
        self.tasks.contains_key(name)
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

struct TaskInfo {
    handle: JoinHandle<Result<()>>,
    #[allow(dead_code)] // May be used for future task cancellation functionality
    cancel_token: CancellationToken,
}
