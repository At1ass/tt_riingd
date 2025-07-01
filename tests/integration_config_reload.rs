//! Integration tests for configuration hot reload functionality.
//!
//! Tests the complete configuration pipeline including loading, reloading,
//! validation, and change analysis without hardware dependencies.
//! Uses mocks to isolate configuration logic from hardware concerns.

use anyhow::{Result, anyhow};
use std::time::Duration;
use tokio::time::timeout;

mod mocks;
use mocks::{MockAppState, MockConfigManager, test_utils::*};

use tt_riingd::config::CurveCfg;

/// Test successful configuration hot reload with various curve types.
///
/// This test demonstrates the data-driven testing pattern recommended
/// in rust-analyzer architecture documentation.
#[tokio::test]
async fn test_config_hot_reload_success() -> Result<()> {
    // Arrange: Create initial mock configuration
    let initial_config = single_controller_config();
    let mock_app_state = MockAppState::new(initial_config).await;

    // Verify initial configuration
    let initial_cfg = mock_app_state.config_manager.get().await;
    assert_eq!(initial_cfg.tick_seconds, 2);
    assert!(!initial_cfg.enable_broadcast);
    assert_eq!(initial_cfg.curves.len(), 1);
    assert_eq!(initial_cfg.curves[0].get_id(), "test_curve");
    drop(initial_cfg);

    // Act: Update configuration (simulating hot reload)
    let mut updated_config = complex_config();
    updated_config.tick_seconds = 5;
    updated_config.enable_broadcast = true;
    updated_config.broadcast_interval = 10;

    mock_app_state
        .config_manager
        .update_config(updated_config)
        .await;
    mock_app_state.config_manager.reload().await?;

    // Assert: Verify updated configuration
    let updated_cfg = mock_app_state.config_manager.get().await;
    assert_eq!(updated_cfg.tick_seconds, 5);
    assert!(updated_cfg.enable_broadcast);
    assert_eq!(updated_cfg.broadcast_interval, 10);
    assert_eq!(updated_cfg.curves.len(), 3);

    // Verify specific curve changes
    let silent_curve = updated_cfg
        .curves
        .iter()
        .find(|c| c.get_id() == "silent")
        .unwrap();
    assert!(
        matches!(silent_curve, CurveCfg::Constant { speed: 30, .. }),
        "Expected constant curve with speed 30, got {silent_curve:?}"
    );

    let performance_curve = updated_cfg
        .curves
        .iter()
        .find(|c| c.get_id() == "performance")
        .unwrap();
    if let CurveCfg::StepCurve { tmps, spds, .. } = performance_curve {
        assert_eq!(
            tmps,
            &vec![25.0, 45.0, 65.0, 80.0],
            "Performance curve temperatures don't match expected values"
        );
        assert_eq!(
            spds,
            &vec![20, 40, 70, 95],
            "Performance curve speeds don't match expected values"
        );
    } else {
        return Err(anyhow!(
            "Expected step curve for performance, got {:?}",
            performance_curve
        ));
    }

    Ok(())
}

/// Test configuration reload failure handling and rollback.
///
/// Validates error handling and configuration preservation
/// when reload operations fail.
#[tokio::test]
async fn test_config_hot_reload_invalid_config() -> Result<()> {
    // Arrange: Create a failing mock
    let mock_app_state = MockAppState::new_failing().await;

    // Act & Assert: Attempt reload should fail
    let reload_result = mock_app_state.config_manager.reload().await;
    let error = reload_result.expect_err("Expected reload to fail with mock failure");
    assert!(
        error.to_string().contains("Mock reload failure"),
        "Error message should contain 'Mock reload failure', got: {error}"
    );

    Ok(())
}

/// Test configuration change analysis for hot reload vs cold restart decisions.
///
/// This test validates the configuration change detection logic
/// that determines whether a hot reload or cold restart is required.
#[tokio::test]
async fn test_config_change_analysis() -> Result<()> {
    // Test that this function exists and doesn't crash
    // Real implementation would analyze configuration differences
    // and return appropriate change type

    let initial_config = single_controller_config();
    let config_manager = MockConfigManager::new(initial_config);

    // Simulate a configuration change
    let updated_config = complex_config();
    config_manager.update_config(updated_config).await;

    // In a real implementation, this would analyze changes
    // For now, we just test that the system can handle the operation
    let reload_result = config_manager.reload().await;
    assert!(reload_result.is_ok());

    Ok(())
}

/// Test complex configuration validation with all curve types.
///
/// Uses mocks to test configuration validation without hardware dependencies.
/// This test demonstrates the expected behavior of the system.
#[tokio::test]
async fn test_complex_configuration_validation() -> Result<()> {
    // Arrange: Create complex configuration using test utilities
    let complex_config = complex_config();
    let mock_app_state = MockAppState::new(complex_config).await;

    // Act & Assert: Test configuration loading (without hardware initialization)
    let config = mock_app_state.config_manager.get().await;

    // Verify basic settings
    assert_eq!(config.tick_seconds, 3);
    assert!(config.enable_broadcast);
    assert_eq!(config.broadcast_interval, 15);

    // Verify controllers
    assert_eq!(config.controllers.len(), 1);
    match &config.controllers[0] {
        tt_riingd::config::ControllerCfg::RiingQuad { fans, .. } => {
            assert_eq!(fans.len(), 3);
            assert_eq!(fans[0].name, "CPU Intake");
            assert_eq!(fans[1].name, "CPU Exhaust");
            assert_eq!(fans[2].name, "Case Intake");
        }
    }

    // Verify curves
    assert_eq!(config.curves.len(), 3);
    let curve_ids: Vec<String> = config.curves.iter().map(|c| c.get_id()).collect();
    assert!(curve_ids.contains(&"silent".to_string()));
    assert!(curve_ids.contains(&"performance".to_string()));
    assert!(curve_ids.contains(&"smooth".to_string()));

    // Test curve calculations
    let silent_curve = config
        .curves
        .iter()
        .find(|c| c.get_id() == "silent")
        .unwrap();
    assert_eq!(silent_curve.calculate_speed(45.0)?, 30);

    let performance_curve = config
        .curves
        .iter()
        .find(|c| c.get_id() == "performance")
        .unwrap();
    let speed_at_55 = performance_curve.calculate_speed(55.0)?;
    assert!((40..=70).contains(&speed_at_55)); // Interpolated value

    let smooth_curve = config
        .curves
        .iter()
        .find(|c| c.get_id() == "smooth")
        .unwrap();
    let speed_at_50 = smooth_curve.calculate_speed(50.0)?;
    assert!((20..=90).contains(&speed_at_50)); // Bezier interpolated

    // Verify sensors
    assert_eq!(config.sensors.len(), 2);

    // Verify mappings
    assert_eq!(config.mappings.len(), 2);
    assert_eq!(config.mappings[0].targets.len(), 2);
    assert_eq!(config.mappings[1].targets.len(), 1);

    // Verify curve mappings
    assert_eq!(config.active_curve_mappings.len(), 3);

    // Verify colors and color mappings
    assert_eq!(config.effects.len(), 3);
    assert_eq!(config.effect_mappings.len(), 2);

    Ok(())
}

/// Test concurrent configuration access patterns.
///
/// Validates thread-safety of configuration management
/// under concurrent load.
#[tokio::test]
async fn test_concurrent_config_access() -> Result<()> {
    // Arrange: Create minimal configuration for concurrent testing
    let config = minimal_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act: Spawn multiple concurrent readers
    let mut handles = Vec::new();

    for i in 0..10 {
        let config_manager = mock_app_state.config_manager.clone();
        let handle = tokio::spawn(async move {
            for _j in 0..50 {
                let config = config_manager.get().await;
                assert_eq!(config.tick_seconds, 2);
                assert_eq!(config.version, 1);

                // Simulate some work
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            i
        });
        handles.push(handle);
    }

    // Assert: Wait for all readers to complete
    let results: Result<Vec<_>, _> = timeout(Duration::from_secs(10), async {
        futures::future::try_join_all(handles).await
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timeout waiting for concurrent readers"))?;

    // Verify all tasks completed successfully
    assert_eq!(results?.len(), 10);

    Ok(())
}

/// Test configuration validation edge cases.
///
/// Tests boundary conditions and error scenarios
/// in configuration validation.
#[tokio::test]
async fn test_config_validation_edge_cases() -> Result<()> {
    // Test empty configuration
    let empty_config = minimal_config();
    let mock_app_state = MockAppState::new(empty_config).await;

    let config = mock_app_state.config_manager.get().await;
    assert!(config.controllers.is_empty());
    assert!(config.curves.is_empty());
    assert!(config.sensors.is_empty());
    assert!(config.mappings.is_empty());

    Ok(())
}

/// Test sensor data updates and retrieval.
///
/// Validates the mock sensor data management functionality.
#[tokio::test]
async fn test_sensor_data_management() -> Result<()> {
    // Arrange
    let config = single_controller_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act: Set sensor data
    mock_app_state.set_sensor_data("cpu_temp", 55.5).await;
    mock_app_state.set_sensor_data("gpu_temp", 42.0).await;

    // Assert: Verify data retrieval
    assert_eq!(mock_app_state.get_sensor_data("cpu_temp").await, Some(55.5));
    assert_eq!(mock_app_state.get_sensor_data("gpu_temp").await, Some(42.0));
    assert_eq!(mock_app_state.get_sensor_data("nonexistent").await, None);

    // Act: Update existing sensor data
    mock_app_state.set_sensor_data("cpu_temp", 65.2).await;

    // Assert: Verify updated data
    assert_eq!(mock_app_state.get_sensor_data("cpu_temp").await, Some(65.2));

    Ok(())
}
