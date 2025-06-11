use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use notify::{Event, EventHandler, RecursiveMode, Watcher, recommended_watcher};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app_context::AppState,
    event::{ConfigChangeType, Event as AppEvent, EventBus},
    providers::traits::ServiceProvider,
    task_manager::TaskManager,
};

/// Configuration file monitoring service provider.
///
/// Provides a non-critical service that monitors the configuration file for
/// changes using efficient filesystem notifications (inotify on Linux) and
/// triggers configuration reloads when modifications are detected.
/// This enables hot-reloading of configuration without daemon restart.
///
/// # Priority and Criticality
///
/// - **Priority**: 6 (medium)
/// - **Critical**: No (optional service)
///
/// # Features
///
/// - Efficient filesystem event monitoring (inotify/kqueue)
/// - Automatic configuration reload on file changes
/// - Configuration change event publishing
/// - Graceful handling of file system errors
/// - Debouncing for rapid file changes
/// - Cancel-safe async design
///
/// # Implementation
///
/// Uses the `notify` crate v8.0.0 which provides cross-platform filesystem
/// notifications with native backends:
/// - Linux: inotify
/// - macOS: FSEvents/kqueue
/// - Windows: ReadDirectoryChangesW
///
/// The implementation follows modern async Rust patterns with proper
/// cancellation safety and structured concurrency.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::ConfigWatcherServiceProvider;
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = ConfigWatcherServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub struct ConfigWatcherServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
}

impl ConfigWatcherServiceProvider {
    /// Creates a new configuration watcher service provider.
    ///
    /// # Arguments
    ///
    /// * `state` - Shared application state containing config manager
    /// * `event_bus` - Event bus for publishing configuration changes
    pub fn new(state: Arc<AppState>, event_bus: EventBus) -> Self {
        Self { state, event_bus }
    }
}

#[async_trait]
impl ServiceProvider for ConfigWatcherServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_config_watcher_service(state, event_bus, cancel_token).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "ConfigWatcherService"
    }

    fn priority(&self) -> i32 {
        6
    }

    fn is_critical(&self) -> bool {
        false
    }
}

/// Event handler for filesystem notifications that implements cancel-safe processing.
#[derive(Debug)]
struct AsyncEventHandler {
    sender: mpsc::UnboundedSender<notify::Result<Event>>,
}

impl AsyncEventHandler {
    fn new(sender: mpsc::UnboundedSender<notify::Result<Event>>) -> Self {
        Self { sender }
    }
}

impl EventHandler for AsyncEventHandler {
    fn handle_event(&mut self, event: notify::Result<Event>) {
        if let Err(e) = self.sender.send(event) {
            error!("Failed to send filesystem event to async handler: {}", e);
        }
    }
}

/// Configuration file monitoring service implementation.
///
/// Uses `notify` v8.0.0 with modern async patterns to efficiently monitor
/// the configuration file for changes and triggers reload events when
/// modifications are detected.
///
/// # Cancel Safety
///
/// This implementation is designed to be cancel-safe:
/// - No state is lost when the future is dropped
/// - Proper cleanup of file watchers
/// - Graceful handling of channel closures
async fn run_config_watcher_service(
    state: Arc<AppState>,
    event_bus: EventBus,
    cancel_token: CancellationToken,
) -> Result<()> {
    let config_path = state.config_manager().path().to_path_buf();
    info!("Config watcher started for: {}", config_path.display());

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let event_handler = AsyncEventHandler::new(event_tx);

    let mut watcher = recommended_watcher(event_handler)?;

    let watch_path = if let Some(parent) = config_path.parent() {
        parent.to_path_buf()
    } else {
        config_path.clone()
    };

    watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;
    info!("Watching directory: {}", watch_path.display());

    let mut debounce_interval = tokio::time::interval(Duration::from_millis(2000));
    debounce_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut has_pending_event = false;

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Config watcher service cancelled");
                break;
            }

            event_result = event_rx.recv() => {
                match event_result {
                    Some(Ok(event)) => {
                        debug!("Received filesystem event: {:?}", event);
                        debug!("Event kind: {:?}", event.kind);
                        debug!("Event paths: {:?}", event.paths);

                        let affects_config = event.paths.iter().any(|path| {
                            let is_exact_match = path == &config_path;
                            let is_filename_match = path.file_name() == config_path.file_name();
                            debug!("Checking path: {:?} - exact_match: {}, filename_match: {}",
                                   path, is_exact_match, is_filename_match);
                            is_exact_match || is_filename_match
                        });

                        // Only react to events that indicate actual file modifications or creation
                        let is_relevant_event = event.kind.is_modify() || event.kind.is_create();

                        if affects_config && is_relevant_event {
                            debug!("Event affects config file and is relevant, marking for debounced reload");
                            has_pending_event = true;
                        } else {
                            debug!("Event does not affect config file or is not relevant (kind: {:?}), ignoring", event.kind);
                        }
                    }
                    Some(Err(e)) => {
                        warn!("Filesystem watcher error: {}", e);
                    }
                    None => {
                        warn!("Filesystem event channel closed, exiting");
                        break;
                    }
                }
            }

            _ = debounce_interval.tick(), if has_pending_event => {
                debug!("Debounce interval elapsed, processing config change analysis");
                has_pending_event = false;

                if config_path.exists() {
                    info!("Configuration file change detected, analyzing changes...");

                    match state.config_manager().analyze_config_changes().await {
                        Ok(change_type) => {
                            match &change_type {
                                ConfigChangeType::HotReload => {
                                    info!("Hot-reloadable changes detected");
                                    if let Err(e) = event_bus.publish(AppEvent::ConfigChangeDetected(change_type)) {
                                        error!("Failed to publish config change event: {}", e);
                                    } else {
                                        info!("Published hot-reload configuration change event");
                                    }
                                }
                                ConfigChangeType::ColdRestart { changed_sections } => {
                                    warn!("Hardware configuration changes detected in sections: {:?}", changed_sections);
                                    warn!("These changes require daemon restart to take effect");
                                    info!("Configuration will not be reloaded to prevent hardware conflicts");

                                    if let Err(e) = event_bus.publish(AppEvent::ConfigChangeDetected(change_type)) {
                                        error!("Failed to publish config change event: {}", e);
                                    } else {
                                        info!("Published cold-restart configuration change event");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to analyze configuration changes: {}", e);
                        }
                    }
                } else {
                    warn!("Configuration file {} no longer exists", config_path.display());
                }
            }
        }
    }

    if let Err(e) = watcher.unwatch(&watch_path) {
        warn!("Failed to unwatch path during cleanup: {}", e);
    }

    info!("Config watcher service stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::time::{sleep, timeout};

    async fn create_mock_app_state() -> Arc<AppState> {
        let config = Config::default();
        let temp_file = NamedTempFile::new().unwrap();
        let config_manager =
            crate::config::ConfigManager::new(config, temp_file.path().to_path_buf());
        Arc::new(AppState::new(config_manager).await.unwrap())
    }

    #[tokio::test]
    async fn test_config_watcher_service_provider_creation() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();

        let provider = ConfigWatcherServiceProvider::new(state, event_bus);

        assert_eq!(provider.name(), "ConfigWatcherService");
        assert_eq!(provider.priority(), 6);
        assert!(!provider.is_critical());
    }

    #[tokio::test]
    async fn test_config_watcher_service_starts() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();
        let provider = ConfigWatcherServiceProvider::new(state, event_bus);

        let mut task_manager = TaskManager::new();
        let result = provider.start(&mut task_manager).await;

        assert!(result.is_ok());
        assert_eq!(task_manager.active_count(), 1);

        let _ = task_manager.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_config_file_change_detection() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();

        let config = Config::default();
        let config_manager = crate::config::ConfigManager::new(config, config_path.clone());
        let state = Arc::new(AppState::new(config_manager).await.unwrap());

        let event_bus = EventBus::new();
        let mut event_rx = event_bus.subscribe();

        let provider = ConfigWatcherServiceProvider::new(state, event_bus);
        let mut task_manager = TaskManager::new();

        // Start the service
        provider.start(&mut task_manager).await.unwrap();

        // Give the watcher more time to start and set up file system monitoring
        sleep(Duration::from_millis(500)).await;

        // Write to the config file to trigger an event
        std::fs::write(
            &config_path,
            "version: 1\nfans: []\ncontrollers: []\nmappings: []\ncolor_mappings: []\n",
        )
        .unwrap();

        // Wait for the config reload event with longer timeout
        let event_result = timeout(Duration::from_secs(5), event_rx.recv()).await;

        if event_result.is_err() {
            eprintln!("Timeout waiting for config reload event");
            // Try to trigger another event
            std::fs::write(&config_path, "# Modified\nversion: 1\nfans: []\ncontrollers: []\nmappings: []\ncolor_mappings: []\n").unwrap();

            // Wait again with shorter timeout
            let retry_result = timeout(Duration::from_secs(2), event_rx.recv()).await;
            assert!(
                retry_result.is_ok(),
                "Failed to receive config reload event even after retry"
            );

            match retry_result.unwrap() {
                Ok(AppEvent::ConfigChangeDetected(_)) => {
                    // Test passed - we received the expected event
                }
                other => panic!("Expected ConfigChangeDetected event, got: {:?}", other),
            }
        } else {
            match event_result.unwrap() {
                Ok(AppEvent::ConfigChangeDetected(_)) => {
                    // Test passed - we received the expected event
                }
                other => panic!("Expected ConfigChangeDetected event, got: {:?}", other),
            }
        }

        let _ = task_manager.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_config_watcher_graceful_shutdown() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();
        let provider = ConfigWatcherServiceProvider::new(state, event_bus);

        let mut task_manager = TaskManager::new();
        provider.start(&mut task_manager).await.unwrap();

        // Verify task is running
        assert_eq!(task_manager.active_count(), 1);

        // Shutdown should complete without errors
        let shutdown_result = task_manager.shutdown_all().await;
        assert!(shutdown_result.is_ok());

        // Verify task is stopped
        assert_eq!(task_manager.active_count(), 0);
    }

    #[tokio::test]
    async fn test_debouncing_with_modern_patterns() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();

        let config = Config::default();
        let config_manager = crate::config::ConfigManager::new(config, config_path.clone());
        let state = Arc::new(AppState::new(config_manager).await.unwrap());

        let event_bus = EventBus::new();
        let mut event_rx = event_bus.subscribe();

        let provider = ConfigWatcherServiceProvider::new(state, event_bus);
        let mut task_manager = TaskManager::new();

        provider.start(&mut task_manager).await.unwrap();
        sleep(Duration::from_millis(500)).await;

        // Make rapid file changes
        for i in 0..5 {
            std::fs::write(&config_path, format!("# Change {}\nversion: 1\nfans: []\ncontrollers: []\nmappings: []\ncolor_mappings: []\n", i)).unwrap();
            sleep(Duration::from_millis(50)).await; // Very rapid changes
        }

        // Should receive at most 2 events due to debouncing
        let mut event_count = 0;

        // Wait for events with timeout
        while let Ok(Ok(_)) = timeout(Duration::from_millis(1200), event_rx.recv()).await {
            event_count += 1;
            if event_count >= 3 {
                break; // Stop if we get too many events
            }
        }

        // Due to debouncing (500ms), we shouldn't get an event for every change
        assert!(
            event_count <= 2,
            "Received {} events, expected <= 2 due to debouncing",
            event_count
        );

        let _ = task_manager.shutdown_all().await;
    }
}
