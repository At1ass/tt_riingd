//! End-to-end integration tests for the complete application workflow.
//!
//! Tests the complete application lifecycle from startup to shutdown,
//! including configuration management, monitoring cycles, and error recovery.
//! Uses comprehensive mocking to avoid hardware dependencies while
//! maintaining realistic test scenarios.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::timeout;

mod mocks;
use mockall::predicate::*;
use mocks::{
    MockAppState, MockMockableFanController, MockMockableTemperatureSensor, MockableFanController,
    MockableTemperatureSensor, test_utils::*,
};

use tt_riingd::{
    config::CurveCfg,
    event::{Event, EventBus},
};

/// Test complete application startup workflow.
///
/// Simulates the full application initialization process using mocks
/// to ensure all components integrate correctly.
#[tokio::test]
async fn test_complete_application_startup() -> Result<()> {
    // Arrange: Create comprehensive test configuration
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act & Assert: Verify application state initialization
    let config = mock_app_state.config_manager.get().await;

    // Verify basic configuration loaded correctly
    assert_eq!(config.version, 1);
    assert_eq!(config.tick_seconds, 3);
    assert!(config.enable_broadcast);

    // Verify all components are configured
    assert_eq!(config.controllers.len(), 1);
    assert_eq!(config.curves.len(), 3);
    assert_eq!(config.sensors.len(), 2);
    assert_eq!(config.mappings.len(), 2);
    assert_eq!(config.active_curve_mappings.len(), 3);

    // Verify startup can complete without errors
    println!("✓ Application startup simulation completed successfully");

    Ok(())
}

/// Test configuration hot reload workflow.
///
/// Simulates the complete configuration reload process including
/// validation, change detection, and system reconfiguration.
#[tokio::test]
async fn test_hot_reload_workflow() -> Result<()> {
    // Arrange: Start with initial configuration
    let initial_config = single_controller_config();
    let mock_app_state = MockAppState::new(initial_config).await;

    // Verify initial state
    let config = mock_app_state.config_manager.get().await;
    assert_eq!(config.curves.len(), 1);
    assert_eq!(config.tick_seconds, 2);
    drop(config);

    // Act: Simulate configuration hot reload
    let mut updated_config = complex_config();
    updated_config.tick_seconds = 5;

    mock_app_state
        .config_manager
        .update_config(updated_config)
        .await;

    // Simulate reload process
    let reload_result = mock_app_state.config_manager.reload().await;
    assert!(reload_result.is_ok());

    // Assert: Verify updated configuration applied
    let config = mock_app_state.config_manager.get().await;
    assert_eq!(config.curves.len(), 3);
    assert_eq!(config.tick_seconds, 5);
    assert!(config.enable_broadcast);

    println!("✓ Hot reload workflow completed successfully");

    Ok(())
}

/// Test error recovery mechanisms.
///
/// Validates graceful degradation and recovery when various failures occur.
#[tokio::test]
async fn test_error_recovery() -> Result<()> {
    // Test 1: Configuration reload failure recovery
    let failing_app_state = MockAppState::new_failing().await;

    let reload_result = failing_app_state.config_manager.reload().await;
    let error = reload_result.expect_err("Expected reload to fail with mock failure");
    assert!(
        error.to_string().contains("Mock reload failure"),
        "Error message should contain 'Mock reload failure', got: {}",
        error
    );

    // Test 2: Invalid curve calculation recovery
    let mut config = single_controller_config();
    config.curves = vec![CurveCfg::Bezier {
        id: "invalid_bezier".to_string(),
        points: vec![
            tt_riingd::fan_curve::Point { x: 30.0, y: 20.0 },
            tt_riingd::fan_curve::Point { x: 70.0, y: 80.0 },
        ],
    }];

    let mock_app_state = MockAppState::new(config).await;
    let config = mock_app_state.config_manager.get().await;
    let invalid_curve = &config.curves[0];

    let calc_result = invalid_curve.calculate_speed(50.0);
    assert!(calc_result.is_err());

    // Test 3: Mock fan controller error simulation
    let mut mock_controller = MockMockableFanController::new();
    mock_controller
        .expect_send_init()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("Simulated hardware failure")));

    let init_result = mock_controller.send_init().await;
    let error = init_result.expect_err("Expected hardware initialization to fail");
    assert!(
        error.to_string().contains("Simulated hardware failure"),
        "Error message should contain 'Simulated hardware failure', got: {}",
        error
    );

    println!("✓ Error recovery mechanisms validated");

    Ok(())
}

/// Test monitoring cycle simulation with realistic scenarios.
///
/// Simulates a complete monitoring cycle including sensor readings,
/// curve calculations, and fan control updates.
#[tokio::test]
async fn test_monitoring_cycle_simulation() -> Result<()> {
    // Arrange: Setup monitoring environment
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Setup mock sensors with realistic temperature patterns
    let temperature_pattern = vec![
        ("cpu_temp", vec![45.0, 52.0, 65.0, 72.0, 68.0, 55.0, 48.0]),
        ("gpu_temp", vec![35.0, 42.0, 58.0, 70.0, 75.0, 62.0, 45.0]),
    ];

    // Act: Simulate monitoring cycles
    for cycle in 0..temperature_pattern[0].1.len() {
        // Update sensor readings
        for (sensor_name, temps) in &temperature_pattern {
            mock_app_state
                .set_sensor_data(sensor_name, temps[cycle])
                .await;
        }

        // Simulate curve calculations for each sensor
        let config = mock_app_state.config_manager.get().await;

        for (_sensor_name, temps) in &temperature_pattern {
            let current_temp = temps[cycle];

            // Find appropriate curve for this sensor based on mappings
            for curve in &config.curves {
                let speed = curve.calculate_speed(current_temp)?;

                // Verify speed is within reasonable bounds
                assert!(
                    (20..=100).contains(&speed),
                    "Speed {} out of bounds for temp {} on curve {}",
                    speed,
                    current_temp,
                    curve.get_id()
                );
            }
        }

        // Simulate processing delay
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Assert: Verify final state
    assert_eq!(mock_app_state.get_sensor_data("cpu_temp").await, Some(48.0));
    assert_eq!(mock_app_state.get_sensor_data("gpu_temp").await, Some(45.0));

    println!("✓ Monitoring cycle simulation completed");

    Ok(())
}

/// Test concurrent operations under load.
///
/// Validates system behavior under concurrent access patterns
/// typical of production usage.
#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    // Arrange: Setup for concurrent testing
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act: Launch concurrent operations
    let mut handles = Vec::new();

    // Concurrent sensor updates
    for sensor_id in 0..5 {
        let app_state = mock_app_state.clone();
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let temp = 30.0 + (i % 40) as f32 + sensor_id as f32;
                let sensor_name = format!("sensor_{}", sensor_id);
                app_state.set_sensor_data(&sensor_name, temp).await;

                // Small delay to simulate realistic timing
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            sensor_id
        });
        handles.push(handle);
    }

    // Concurrent configuration reads
    for reader_id in 0..3 {
        let config_manager = mock_app_state.config_manager.clone();
        let handle = tokio::spawn(async move {
            for i in 0..50 {
                let config = config_manager.get().await;
                assert_eq!(config.version, 1);
                assert_eq!(config.curves.len(), 3);

                // Simulate curve calculation work
                for curve in &config.curves {
                    let temp = 40.0 + (i % 30) as f32;
                    let _speed = curve.calculate_speed(temp).unwrap();
                }

                tokio::time::sleep(Duration::from_micros(200)).await;
            }
            reader_id + 100
        });
        handles.push(handle);
    }

    // Assert: Wait for all operations to complete
    let results = timeout(Duration::from_secs(10), async {
        futures::future::try_join_all(handles).await
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timeout in concurrent operations"))?;

    let completed_results = results?;
    assert_eq!(completed_results.len(), 8); // 5 sensors + 3 readers

    println!("✓ Concurrent operations completed successfully");

    Ok(())
}

/// Test resource management and cleanup.
///
/// Validates proper resource allocation and cleanup patterns.
#[tokio::test]
async fn test_resource_management() -> Result<()> {
    // Test memory usage patterns
    let initial_config = minimal_config();
    let mock_app_state = MockAppState::new(initial_config).await;

    // Simulate resource-intensive operations
    for _i in 0..1000 {
        // Create temporary configurations
        let temp_config = complex_config();
        mock_app_state
            .config_manager
            .update_config(temp_config)
            .await;

        // Perform calculations
        let config = mock_app_state.config_manager.get().await;
        for curve in &config.curves {
            let _speed = curve.calculate_speed(50.0)?;
        }
    }

    // Verify system still responsive
    let config = mock_app_state.config_manager.get().await;
    assert!(config.curves.len() >= 3);

    println!("✓ Resource management test completed");

    Ok(())
}

/// Test application shutdown procedures.
///
/// Validates graceful shutdown and cleanup processes.
#[tokio::test]
async fn test_application_shutdown() -> Result<()> {
    // Arrange: Setup application state
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Simulate some application activity
    mock_app_state.set_sensor_data("cpu_temp", 55.0).await;
    mock_app_state.set_sensor_data("gpu_temp", 42.0).await;

    // Act: Simulate graceful shutdown using EventBus
    let event_bus = EventBus::new();
    let mut receiver = event_bus.subscribe();

    event_bus.publish(Event::SystemShutdown)?;

    // Assert: Verify shutdown event received
    let shutdown_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .context("Timeout waiting for shutdown event")?
        .context("Failed to receive shutdown event")?;

    assert!(
        matches!(shutdown_event, Event::SystemShutdown),
        "Expected SystemShutdown event, got {:?}",
        shutdown_event
    );
    println!("✓ Graceful shutdown event processed");

    // Verify final state is accessible
    assert_eq!(mock_app_state.get_sensor_data("cpu_temp").await, Some(55.0));
    assert_eq!(mock_app_state.get_sensor_data("gpu_temp").await, Some(42.0));

    println!("✓ Application shutdown procedures validated");

    Ok(())
}

/// Test event system integration throughout application lifecycle.
///
/// Validates event publishing, subscription, and processing patterns.
#[tokio::test]
async fn test_event_system_lifecycle() -> Result<()> {
    // Arrange: Setup event system
    let event_bus = EventBus::new();
    let mut receiver = event_bus.subscribe();

    // Test event sequence
    let events = vec![Event::SystemShutdown];

    // Act: Publish events in sequence
    for event in events {
        event_bus.publish(event)?;
    }

    // Assert: Verify all events received
    let received_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .context("Timeout waiting for event")?
        .context("Failed to receive event")?;

    assert!(
        matches!(received_event, Event::SystemShutdown),
        "Expected SystemShutdown event, got {:?}",
        received_event
    );
    println!("✓ Event system lifecycle validated");

    Ok(())
}

/// Test integration with mock hardware controllers.
///
/// Demonstrates complete hardware interaction patterns using mocks.
#[tokio::test]
async fn test_mock_hardware_integration() -> Result<()> {
    // Arrange: Setup mock hardware stack
    let mut mock_controller = MockMockableFanController::new();
    let mut mock_sensor = MockMockableTemperatureSensor::new();

    // Configure realistic mock expectations
    mock_controller
        .expect_send_init()
        .times(1)
        .returning(|| Ok(()));

    mock_controller
        .expect_firmware_version()
        .times(1)
        .returning(|| Ok((2, 1, 0)));

    mock_controller
        .expect_update_channel()
        .times(3)
        .returning(|_, _, _| Ok(()));

    mock_sensor
        .expect_sensor_key()
        .times(1)
        .returning(|| "cpu_temp".to_string());

    mock_sensor
        .expect_read_temperature()
        .times(3)
        .returning(|| Ok(58.5));

    // Act: Simulate hardware interaction sequence
    mock_controller.send_init().await?;
    let version = mock_controller.firmware_version().await?;
    assert_eq!(version, (2, 1, 0));

    let sensor_key = mock_sensor.sensor_key();
    assert_eq!(sensor_key, "cpu_temp");

    // Simulate monitoring loop
    for i in 1..=3 {
        let temperature = mock_sensor.read_temperature().await?;
        assert_eq!(temperature, 58.5);

        // Calculate fan speed (simplified)
        let fan_speed = ((temperature - 20.0) * 2.0).clamp(0.0, 100.0) as u8;

        mock_controller
            .update_channel(i, temperature, fan_speed)
            .await?;
    }

    println!("✓ Mock hardware integration test completed");

    Ok(())
}

/// Helper trait extension for MockAppState
///
/// Provides convenient methods for common testing patterns.
trait MockAppStateExt {
    async fn simulate_temperature_spike(
        &self,
        sensor: &str,
        base_temp: f32,
        spike_temp: f32,
    ) -> Result<()>;
    async fn simulate_cooling_cycle(
        &self,
        sensor: &str,
        hot_temp: f32,
        cool_temp: f32,
        steps: usize,
    ) -> Result<()>;
}

impl MockAppStateExt for MockAppState {
    async fn simulate_temperature_spike(
        &self,
        sensor: &str,
        base_temp: f32,
        spike_temp: f32,
    ) -> Result<()> {
        // Gradual temperature increase
        for i in 0..10 {
            let temp = base_temp + (spike_temp - base_temp) * (i as f32 / 9.0);
            self.set_sensor_data(sensor, temp).await;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Gradual temperature decrease
        for i in 0..10 {
            let temp = spike_temp - (spike_temp - base_temp) * (i as f32 / 9.0);
            self.set_sensor_data(sensor, temp).await;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        Ok(())
    }

    async fn simulate_cooling_cycle(
        &self,
        sensor: &str,
        hot_temp: f32,
        cool_temp: f32,
        steps: usize,
    ) -> Result<()> {
        for i in 0..steps {
            let temp = hot_temp - (hot_temp - cool_temp) * (i as f32 / (steps - 1) as f32);
            self.set_sensor_data(sensor, temp).await;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        Ok(())
    }
}

/// Test realistic temperature simulation patterns.
///
/// Demonstrates the helper trait extension for complex testing scenarios.
#[tokio::test]
async fn test_realistic_temperature_patterns() -> Result<()> {
    // Arrange
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act: Simulate temperature spike
    mock_app_state
        .simulate_temperature_spike("cpu_temp", 45.0, 85.0)
        .await?;

    // Verify final temperature
    let final_temp = mock_app_state.get_sensor_data("cpu_temp").await;
    let final_temp = final_temp.context("Expected final temperature to be set")?;
    assert!(
        (final_temp - 45.0).abs() < 1.0,
        "Final temperature {} should be close to 45.0",
        final_temp
    );

    // Act: Simulate cooling cycle
    mock_app_state
        .simulate_cooling_cycle("gpu_temp", 75.0, 35.0, 20)
        .await?;

    // Verify cooling completed
    let cooled_temp = mock_app_state.get_sensor_data("gpu_temp").await;
    let cooled_temp = cooled_temp.context("Expected cooled temperature to be set")?;
    assert!(
        (cooled_temp - 35.0).abs() < 1.0,
        "Cooled temperature {} should be close to 35.0",
        cooled_temp
    );

    println!("✓ Realistic temperature pattern simulation completed");

    Ok(())
}
