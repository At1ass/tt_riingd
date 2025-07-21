//! Integration tests for monitoring system functionality.
//!
//! Tests the complete monitoring pipeline including sensor reading,
//! curve calculation, fan control, and event processing using mocks
//! to avoid hardware dependencies.

use anyhow::{Context, Result};
use std::time::Duration;

mod mocks;
use mocks::{
    MockAppState, MockMockableFanController, MockMockableTemperatureSensor, MockableFanController,
    MockableTemperatureSensor, test_helpers, test_utils::*,
};

use tt_riingd::{
    config::CurveCfg,
    core::event::{Event, MessageBroker},
};

/// Test basic monitoring cycle with constant curve.
///
/// Demonstrates the data-driven testing pattern for monitoring logic.
#[tokio::test]
async fn test_basic_monitoring_cycle() -> Result<()> {
    // Arrange: Create monitoring setup with constant curve
    let config = single_controller_config();
    let mock_app_state = MockAppState::new(config).await;

    // Set initial sensor data
    mock_app_state.set_sensor_data("test_sensor", 45.0).await;

    // Act & Assert: Verify sensor data is accessible
    let temperature = mock_app_state.get_sensor_data("test_sensor").await;
    assert_eq!(temperature, Some(45.0));

    // Verify curve calculation
    let config = mock_app_state.config_manager.get().await;
    let curve = &config.curves[0];
    let speed = curve.calculate_speed(45.0)?;
    assert_eq!(speed, 50); // Constant curve speed

    Ok(())
}

/// Test step curve monitoring with temperature-based speed calculation.
///
/// Validates interpolation behavior in step curves.
#[tokio::test]
async fn test_step_curve_monitoring() -> Result<()> {
    // Arrange: Create configuration with step curve
    let mut config = single_controller_config();
    config.curves = vec![CurveCfg::StepCurve {
        id: "step_test".to_string(),
        tmps: vec![30.0, 50.0, 70.0],
        spds: vec![25, 50, 75],
    }];

    let mock_app_state = MockAppState::new(config).await;

    // Test various temperature points
    let test_cases = vec![
        (25.0, 25), // Below first point
        (30.0, 25), // At first point
        (40.0, 37), // Interpolated (25 + (50-25)*0.5 = 37.5, rounded down)
        (50.0, 50), // At second point
        (60.0, 62), // Interpolated (50 + (75-50)*0.5 = 62.5, rounded down)
        (70.0, 75), // At third point
        (80.0, 75), // Above last point
    ];

    // Act & Assert: Test curve calculations
    let config = mock_app_state.config_manager.get().await;
    let curve = &config.curves[0];

    for (temp, expected_speed) in test_cases {
        mock_app_state.set_sensor_data("test_sensor", temp).await;
        let calculated_speed = curve.calculate_speed(temp)?;

        // Allow small tolerance for floating point arithmetic
        assert!(
            (calculated_speed as i32 - expected_speed).abs() <= 2,
            "Temperature {temp}: expected ~{expected_speed}, got {calculated_speed}"
        );
    }

    Ok(())
}

/// Test Bezier curve monitoring with smooth transitions.
///
/// Validates Bezier interpolation behavior.
#[tokio::test]
async fn test_bezier_curve_monitoring() -> Result<()> {
    // Arrange: Create configuration with Bezier curve
    let mut config = single_controller_config();
    config.curves = vec![CurveCfg::Bezier {
        id: "bezier_test".to_string(),
        points: vec![
            tt_riingd::fan_curve::Point { x: 30.0, y: 20.0 },
            tt_riingd::fan_curve::Point { x: 45.0, y: 35.0 },
            tt_riingd::fan_curve::Point { x: 65.0, y: 70.0 },
            tt_riingd::fan_curve::Point { x: 80.0, y: 90.0 },
        ],
    }];

    let mock_app_state = MockAppState::new(config).await;

    // Act & Assert: Test Bezier curve calculation
    let config = mock_app_state.config_manager.get().await;
    let curve = &config.curves[0];

    // Test smooth progression
    let temps = vec![30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
    let mut prev_speed = 0;

    for temp in temps {
        mock_app_state.set_sensor_data("test_sensor", temp).await;
        let speed = curve.calculate_speed(temp)?;

        // Bezier should provide smooth, monotonic increase
        assert!(speed >= prev_speed, "Speed should increase monotonically");
        assert!(
            (20..=90).contains(&speed),
            "Speed should be within curve bounds"
        );

        prev_speed = speed;
    }

    Ok(())
}

/// Test multi-sensor monitoring with different mappings.
///
/// Validates handling of multiple sensors with different curve assignments.
#[tokio::test]
async fn test_multi_sensor_monitoring() -> Result<()> {
    // Arrange: Create complex configuration
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Set different sensor temperatures
    mock_app_state.set_sensor_data("cpu_temp", 65.0).await;
    mock_app_state.set_sensor_data("gpu_temp", 45.0).await;

    // Act & Assert: Verify sensor data management
    assert_eq!(mock_app_state.get_sensor_data("cpu_temp").await, Some(65.0));
    assert_eq!(mock_app_state.get_sensor_data("gpu_temp").await, Some(45.0));

    // Verify different curves produce different speeds
    let config = mock_app_state.config_manager.get().await;

    let performance_curve = config
        .curves
        .iter()
        .find(|c| c.get_id() == "performance")
        .unwrap();
    let silent_curve = config
        .curves
        .iter()
        .find(|c| c.get_id() == "silent")
        .unwrap();

    let perf_speed = performance_curve.calculate_speed(65.0)?;
    let silent_speed = silent_curve.calculate_speed(45.0)?;

    assert!(
        perf_speed > silent_speed,
        "Performance curve should be more aggressive"
    );
    assert_eq!(silent_speed, 30); // Constant curve

    Ok(())
}

/// Test MessageBroker integration for monitoring events.
///
/// Validates event publishing and subscription patterns.
#[tokio::test]
async fn test_event_system_integration() -> Result<()> {
    // Arrange: Create MessageBroker for testing
    let event_bus = MessageBroker::new();
    let mut receiver = event_bus.subscribe();

    // Act: Publish a system shutdown event
    event_bus.notify(Event::SystemShutdown)?;

    // Assert: Verify event received
    let received_event = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
        .await
        .context("Timeout waiting for event")?
        .context("Failed to receive event")?;

    assert!(
        matches!(received_event, Event::SystemShutdown),
        "Expected SystemShutdown event, got {received_event:?}"
    );

    Ok(())
}

/// Test monitoring error handling with invalid curves.
///
/// Validates graceful error handling in monitoring scenarios.
#[tokio::test]
async fn test_monitoring_error_handling() -> Result<()> {
    // Arrange: Create configuration with invalid Bezier curve (not enough points)
    let mut config = single_controller_config();
    config.curves = vec![CurveCfg::Bezier {
        id: "invalid_bezier".to_string(),
        points: vec![
            tt_riingd::fan_curve::Point { x: 30.0, y: 20.0 },
            tt_riingd::fan_curve::Point { x: 70.0, y: 80.0 },
        ],
    }];

    let mock_app_state = MockAppState::new(config).await;

    // Act & Assert: Invalid curve should return error
    let config = mock_app_state.config_manager.get().await;
    let invalid_curve = &config.curves[0];
    let result = invalid_curve.calculate_speed(50.0);

    let error = result.expect_err("Expected error for invalid Bezier curve");
    assert!(
        error.to_string().contains("exactly 4 control points"),
        "Error message should mention control points requirement, got: {error}"
    );

    Ok(())
}

/// Test monitoring performance under high frequency updates.
///
/// Validates system performance with rapid sensor updates.
#[tokio::test]
async fn test_monitoring_performance() -> Result<()> {
    // Arrange: Create configuration for performance testing
    let config = single_controller_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act: Perform many rapid updates
    let start = std::time::Instant::now();
    let iterations = 10000;

    for i in 0..iterations {
        let temp = 30.0 + (i % 50) as f32;
        mock_app_state.set_sensor_data("test_sensor", temp).await;

        let config = mock_app_state.config_manager.get().await;
        let curve = &config.curves[0];
        let _speed = curve.calculate_speed(temp)?;
    }

    let duration = start.elapsed();
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();

    // Assert: Performance should be reasonable
    assert!(
        ops_per_sec > 1000.0,
        "Performance too slow: {ops_per_sec} ops/sec"
    );
    println!("Monitoring performance: {ops_per_sec:.0} ops/sec");

    Ok(())
}

/// Test fan controller mock integration.
///
/// Demonstrates proper mock usage for fan controllers.
#[tokio::test]
async fn test_mock_fan_controller() -> Result<()> {
    // Arrange: Create mock fan controller
    let mut mock_controller = MockMockableFanController::new();

    // Set expectations using mockall patterns
    mock_controller
        .expect_send_init()
        .times(1)
        .returning(|| Ok(()));

    mock_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_, _| Ok(()));

    mock_controller
        .expect_firmware_version()
        .times(1)
        .returning(|| Ok((1, 2, 3)));

    // Act & Assert: Test mock behavior
    mock_controller.send_init().await?;
    mock_controller
        .update_speed_batch(
            test_helpers::create_speed_buffer(),
            test_helpers::default_controller_entry(),
        )
        .await?;
    let version = mock_controller.firmware_version().await?;

    assert_eq!(version, (1, 2, 3));

    Ok(())
}

/// Test temperature sensor mock integration.
///
/// Demonstrates controlled temperature readings for testing.
#[tokio::test]
async fn test_mock_temperature_sensor() -> Result<()> {
    // Arrange: Create mock temperature sensor
    let mut mock_sensor = MockMockableTemperatureSensor::new();

    mock_sensor
        .expect_sensor_key()
        .returning(|| "test_sensor".to_string());

    mock_sensor
        .expect_read_temperature()
        .times(3)
        .returning(|| Ok(42.5));

    // Act & Assert: Test mock sensor behavior
    assert_eq!(mock_sensor.sensor_key(), "test_sensor");

    for _ in 0..3 {
        let temp = mock_sensor.read_temperature().await?;
        assert_eq!(temp, 42.5);
    }

    Ok(())
}

/// Test active curves management and lookups.
///
/// Validates curve assignment and retrieval logic.
#[tokio::test]
async fn test_active_curves_management() -> Result<()> {
    // Arrange: Create complex configuration with curve mappings
    let config = complex_config();
    let mock_app_state = MockAppState::new(config).await;

    // Act & Assert: Verify curve mappings are correctly configured
    let config = mock_app_state.config_manager.get().await;

    // Check that each fan has an assigned curve
    assert_eq!(config.active_curve_mappings.len(), 3);

    // Verify specific curve assignments
    let performance_mapping = config
        .active_curve_mappings
        .iter()
        .find(|m| m.curve == "performance")
        .unwrap();
    assert_eq!(performance_mapping.targets.len(), 1);
    assert_eq!(performance_mapping.targets[0].fan_idx, 1);

    let smooth_mapping = config
        .active_curve_mappings
        .iter()
        .find(|m| m.curve == "smooth")
        .unwrap();
    assert_eq!(smooth_mapping.targets[0].fan_idx, 2);

    let silent_mapping = config
        .active_curve_mappings
        .iter()
        .find(|m| m.curve == "silent")
        .unwrap();
    assert_eq!(silent_mapping.targets[0].fan_idx, 3);

    Ok(())
}
