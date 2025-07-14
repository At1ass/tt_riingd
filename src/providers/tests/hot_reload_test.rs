//! Integration tests for hot-reload functionality
//!
//! Tests the Two-Phase Commit protocol implementation and ArcSwap-based
//! configuration updates for both FanColorService and MonitoringService.

use super::super::{fan_color::*, monitoring::*};
use crate::{
    config::{Config, EffectCfg, CurveCfg, Mapping, CurveMapping},
    core::{AppState, Event},
    event::{EventBus, RequestPayload, PrepareConfigUpdateCommand, ServiceType},
    providers::traits::ServiceProvider,
    task_manager::TaskManager,
};
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;
use arc_swap::ArcSwap;

/// Helper to create a test configuration with effects and curves
fn create_test_config_with_hot_reload_data() -> Config {
    Config {
        effects: vec![
            EffectCfg::ConstantColor {
                id: "red".to_string(),
                rgb: [255, 0, 0],
            },
            EffectCfg::ConstantColor {
                id: "blue".to_string(), 
                rgb: [0, 0, 255],
            },
        ],
        curves: vec![
            CurveCfg::Constant {
                id: "low_speed".to_string(),
                speed: 30,
            },
            CurveCfg::Constant {
                id: "high_speed".to_string(),
                speed: 80,
            },
        ],
        ..Default::default()
    }
}

/// Helper to create test AppState with ConfigManager
async fn create_test_app_state() -> Arc<AppState> {
    let config = create_test_config_with_hot_reload_data();
    let config_manager = crate::config::ConfigManager::new(
        config, 
        std::path::PathBuf::from("/tmp/hot_reload_test.yml")
    );
    Arc::new(AppState::new(config_manager).await.unwrap())
}

#[tokio::test]
async fn test_fancolor_hot_reload_prepare_phase() {
    let state = create_test_app_state().await;
    let event_bus = EventBus::new();
    let config_manager = crate::config::ConfigManager::new(
        create_test_config_with_hot_reload_data(),
        std::path::PathBuf::from("/tmp/test.yml")
    );
    
    let provider = FanColorControlServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    let mut task_manager = TaskManager::new();
    provider.start(&mut task_manager).await.unwrap();
    
    // Test prepare phase
    let prepare_command = PrepareConfigUpdateCommand { transaction_id: 123 };
    let result = event_bus.execute(prepare_command).await;
    
    assert!(result.is_ok());
    let responses = result.unwrap();
    assert!(responses.contains_key(&ServiceType::FanColor));
    
    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn test_fancolor_hot_reload_commit_phase() {
    let state = create_test_app_state().await;
    let event_bus = EventBus::new();
    let config_manager = crate::config::ConfigManager::new(
        create_test_config_with_hot_reload_data(),
        std::path::PathBuf::from("/tmp/test.yml")
    );
    
    let provider = FanColorControlServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    let mut task_manager = TaskManager::new();
    provider.start(&mut task_manager).await.unwrap();
    
    // Test prepare + commit workflow
    let prepare_command = PrepareConfigUpdateCommand { transaction_id: 456 };
    event_bus.execute(prepare_command).await.unwrap();
    
    // Commit the transaction
    let commit_event = Event::CommitConfigUpdate { transaction_id: 456 };
    event_bus.notify(commit_event).unwrap();
    
    // Give some time for async processing
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify the configuration was applied
    // (This would require additional testing infrastructure to verify
    // that the new configuration is actually being used)
    
    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn test_monitoring_hot_reload_prepare_phase() {
    let state = create_test_app_state().await;
    let event_bus = EventBus::new();
    let config_manager = crate::config::ConfigManager::new(
        create_test_config_with_hot_reload_data(),
        std::path::PathBuf::from("/tmp/test.yml")
    );
    
    let provider = MonitoringServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    // Note: We'd need to add similar tests for MonitoringService
    // once we fix its constructor signature as well
    
    assert_eq!(provider.name(), "MonitoringService");
}

#[tokio::test]
async fn test_two_phase_commit_protocol() {
    let state = create_test_app_state().await;
    let event_bus = EventBus::new();
    let config_manager = crate::config::ConfigManager::new(
        create_test_config_with_hot_reload_data(),
        std::path::PathBuf::from("/tmp/test.yml")
    );
    
    // Create both services
    let fancolor_provider = FanColorControlServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    let monitoring_provider = MonitoringServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    let mut task_manager = TaskManager::new();
    fancolor_provider.start(&mut task_manager).await.unwrap();
    monitoring_provider.start(&mut task_manager).await.unwrap();
    
    // Test full 2PC workflow
    let transaction_id = 789;
    
    // Phase 1: Prepare
    let prepare_command = PrepareConfigUpdateCommand { transaction_id };
    let prepare_result = event_bus.execute(prepare_command).await;
    
    assert!(prepare_result.is_ok());
    let responses = prepare_result.unwrap();
    
    // Both services should respond successfully
    assert!(responses.contains_key(&ServiceType::FanColor));
    assert!(responses.contains_key(&ServiceType::Monitoring));
    
    // Phase 2: Commit
    let commit_event = Event::CommitConfigUpdate { transaction_id };
    let commit_result = event_bus.notify(commit_event);
    assert!(commit_result.is_ok());
    
    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn test_rollback_scenario() {
    let state = create_test_app_state().await;
    let event_bus = EventBus::new();
    let config_manager = crate::config::ConfigManager::new(
        create_test_config_with_hot_reload_data(),
        std::path::PathBuf::from("/tmp/test.yml")
    );
    
    let provider = FanColorControlServiceProvider::new(
        state.clone(), 
        event_bus.clone(), 
        &config_manager
    ).await;
    
    let mut task_manager = TaskManager::new();
    provider.start(&mut task_manager).await.unwrap();
    
    let transaction_id = 999;
    
    // Prepare phase
    let prepare_command = PrepareConfigUpdateCommand { transaction_id };
    event_bus.execute(prepare_command).await.unwrap();
    
    // Instead of commit, do rollback
    let rollback_event = Event::RollbackConfigUpdate { transaction_id };
    let rollback_result = event_bus.notify(rollback_event);
    assert!(rollback_result.is_ok());
    
    // Verify that the original configuration is still in use
    // (This would require additional testing infrastructure)
    
    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}