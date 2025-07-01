use super::super::config_watcher::ConfigWatcherServiceProvider;
use crate::config::Config;
use crate::core::{
    app_context::AppState,
    event::{Event as AppEvent, EventBus},
    task_manager::TaskManager,
};
use crate::providers::ServiceProvider;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::time::{sleep, timeout};

async fn create_mock_app_state() -> Arc<AppState> {
    let config = Config::default();
    let temp_file = NamedTempFile::new().unwrap();
    let config_manager = crate::config::ConfigManager::new(config, temp_file.path().to_path_buf());
    Arc::new(AppState::new(config_manager).await.unwrap())
}

#[tokio::test]
async fn test_config_watcher_service_provider_creation() {
    let state = create_mock_app_state().await;
    let event_bus = EventBus::new();

    let provider = ConfigWatcherServiceProvider::new(state, event_bus);

    std::assert_eq!(provider.name(), "ConfigWatcherService");
    std::assert_eq!(provider.priority(), 6);
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
    std::assert_eq!(task_manager.active_count(), 1);

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
        std::fs::write(
            &config_path,
            "# Modified\nversion: 1\nfans: []\ncontrollers: []\nmappings: []\ncolor_mappings: []\n",
        )
        .unwrap();

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
            other => panic!("Expected ConfigChangeDetected event, got: {other:?}"),
        }
    } else {
        match event_result.unwrap() {
            Ok(AppEvent::ConfigChangeDetected(_)) => {
                // Test passed - we received the expected event
            }
            other => panic!("Expected ConfigChangeDetected event, got: {other:?}"),
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
    std::assert_eq!(task_manager.active_count(), 1);

    // Shutdown should complete without errors
    let shutdown_result = task_manager.shutdown_all().await;
    assert!(shutdown_result.is_ok());

    // Verify task is stopped
    std::assert_eq!(task_manager.active_count(), 0);
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
        std::fs::write(&config_path, format!("# Change {i}\nversion: 1\nfans: []\ncontrollers: []\nmappings: []\ncolor_mappings: []\n")).unwrap();
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
        "Received {event_count} events, expected <= 2 due to debouncing",
    );

    let _ = task_manager.shutdown_all().await;
}
