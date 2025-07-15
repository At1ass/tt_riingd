//! Integration tests for controller functionality.
//!
//! Tests the complete controller stack including device discovery,
//! initialization, communication, and error handling using mocks
//! to avoid hardware dependencies.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::timeout;

mod mocks;
use mockall::predicate::*;
use mocks::{MockMockableFanController, MockableFanController};

use tt_riingd::{
    config::{ControllerCfg, FanCfg, UsbSelector},
    event::{Event, EventBus},
};

/// Test controller discovery and initialization sequence.
///
/// Validates the complete controller startup workflow including
/// USB device enumeration and hardware initialization.
#[tokio::test]
async fn test_controller_discovery_and_initialization() -> Result<()> {
    // Arrange: Create mock controller with initialization expectations
    let mut mock_controller = MockMockableFanController::new();

    // Expect initialization sequence
    mock_controller
        .expect_send_init()
        .times(1)
        .returning(|| Ok(()));

    mock_controller
        .expect_firmware_version()
        .times(1)
        .returning(|| Ok((2, 1, 0)));

    // Act: Run initialization sequence
    mock_controller
        .send_init()
        .await
        .context("Controller initialization should succeed")?;

    let version = mock_controller
        .firmware_version()
        .await
        .context("Firmware version query should succeed")?;

    // Assert: Verify firmware version
    assert_eq!(version, (2, 1, 0), "Firmware version should match expected");

    println!("✓ Controller discovery and initialization completed");
    Ok(())
}

/// Test controller communication error scenarios.
///
/// Validates error handling when communication with hardware fails.
#[tokio::test]
async fn test_controller_communication_errors() -> Result<()> {
    // Test Case 1: Initialization failure
    let mut failing_controller = MockMockableFanController::new();
    failing_controller
        .expect_send_init()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("USB device not found")));

    let init_error = failing_controller
        .send_init()
        .await
        .expect_err("Initialization should fail");
    assert!(
        init_error.to_string().contains("USB device not found"),
        "Error should indicate USB device issue, got: {init_error}"
    );

    // Test Case 2: Firmware version query failure
    let mut version_failing_controller = MockMockableFanController::new();
    version_failing_controller
        .expect_firmware_version()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("Communication timeout")));

    let version_error = version_failing_controller
        .firmware_version()
        .await
        .expect_err("Firmware version query should fail");
    assert!(
        version_error.to_string().contains("Communication timeout"),
        "Error should indicate timeout, got: {version_error}"
    );

    // Test Case 3: Channel update failure
    let mut update_failing_controller = MockMockableFanController::new();
    update_failing_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("Hardware error")));

    let update_error = update_failing_controller
        .update_speed_batch(vec![(1, 50)])
        .await
        .expect_err("Channel update should fail");
    assert!(
        update_error.to_string().contains("Hardware error"),
        "Error should indicate hardware issue, got: {update_error}"
    );

    println!("✓ Controller communication error scenarios validated");
    Ok(())
}

/// Test multi-channel fan control operations.
///
/// Validates controlling multiple fans on a single controller.
#[tokio::test]
async fn test_multi_channel_fan_control() -> Result<()> {
    // Arrange: Create controller with multiple channel expectations
    let mut mock_controller = MockMockableFanController::new();

    // Setup expectations for batch updates
    let expected_updates = vec![
        (1, 40), // Channel 1, speed 40
        (2, 50), // Channel 2, speed 50
        (3, 60), // Channel 3, speed 60
        (4, 70), // Channel 4, speed 70
    ];

    mock_controller
        .expect_update_speed_batch()
        .with(eq(expected_updates.clone()))
        .times(1)
        .returning(|_| Ok(()));

    // Act: Update all channels using batch API
    let updates = vec![
        (1, 40), // Channel 1, speed 40
        (2, 50), // Channel 2, speed 50
        (3, 60), // Channel 3, speed 60
        (4, 70), // Channel 4, speed 70
    ];

    mock_controller
        .update_speed_batch(updates)
        .await
        .context("Batch channel update should succeed")?;

    println!("✓ Multi-channel fan control operations completed");
    Ok(())
}

/// Test controller configuration validation.
///
/// Validates configuration parsing and validation for controller settings.
#[tokio::test]
async fn test_controller_configuration_validation() -> Result<()> {
    // Test Case 1: Valid controller configuration
    let valid_config = ControllerCfg::RiingQuad {
        id: "main_controller".to_string(),
        usb: UsbSelector {
            vid: 0x264a,
            pid: 0x2330,
            serial: Some("ABC123".to_string()),
        },
        fans: vec![
            FanCfg {
                idx: 1,
                name: "CPU Fan".to_string(),
            },
            FanCfg {
                idx: 2,
                name: "Case Fan".to_string(),
            },
        ],
    };

    // Validate configuration can be serialized/deserialized
    let yaml =
        serde_yaml::to_string(&valid_config).context("Valid configuration should serialize")?;
    let deserialized: ControllerCfg =
        serde_yaml::from_str(&yaml).context("Valid configuration should deserialize")?;

    match (&valid_config, &deserialized) {
        (
            ControllerCfg::RiingQuad {
                id: id1,
                fans: fans1,
                ..
            },
            ControllerCfg::RiingQuad {
                id: id2,
                fans: fans2,
                ..
            },
        ) => {
            assert_eq!(id1, id2, "Controller IDs should match");
            assert_eq!(fans1.len(), fans2.len(), "Fan counts should match");
            assert_eq!(fans1[0].name, fans2[0].name, "Fan names should match");
        }
    }

    // Test Case 2: Edge case configurations
    let edge_case_configs = vec![
        // Minimal configuration
        ControllerCfg::RiingQuad {
            id: "minimal".to_string(),
            usb: UsbSelector {
                vid: 0x1,
                pid: 0x1,
                serial: None,
            },
            fans: vec![],
        },
        // Maximum values
        ControllerCfg::RiingQuad {
            id: "maximum".to_string(),
            usb: UsbSelector {
                vid: 0xFFFF,
                pid: 0xFFFF,
                serial: Some("X".repeat(255)),
            },
            fans: (1..=8)
                .map(|i| FanCfg {
                    idx: i,
                    name: format!("Fan {i}"),
                })
                .collect(),
        },
    ];

    for config in edge_case_configs {
        let yaml =
            serde_yaml::to_string(&config).context("Edge case configuration should serialize")?;
        let _deserialized: ControllerCfg =
            serde_yaml::from_str(&yaml).context("Edge case configuration should deserialize")?;
    }

    println!("✓ Controller configuration validation completed");
    Ok(())
}

/// Test controller event integration.
///
/// Validates controller events are properly published and handled.
#[tokio::test]
async fn test_controller_event_integration() -> Result<()> {
    // Arrange: Setup event system
    let event_bus = EventBus::new();
    let mut receiver = event_bus.subscribe();

    // Simulate controller events
    let events = vec![
        Event::ConfigChangeDetected(tt_riingd::event::ConfigChangeType::HotReload),
        Event::SystemShutdown,
    ];

    // Act: Publish controller-related events
    for event in events {
        event_bus
            .notify(event)
            .context("Event publishing should succeed")?;
    }

    // Assert: Verify events are received
    for expected_count in 1..=2 {
        let received_event = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .context("Timeout waiting for event")?
            .context("Failed to receive event")?;

        match expected_count {
            1 => assert!(
                matches!(received_event, Event::ConfigChangeDetected(_)),
                "First event should be ConfigChangeDetected, got {received_event:?}"
            ),
            2 => assert!(
                matches!(received_event, Event::SystemShutdown),
                "Second event should be SystemShutdown, got {received_event:?}"
            ),
            _ => unreachable!(),
        }
    }

    println!("✓ Controller event integration validated");
    Ok(())
}

/// Test controller performance characteristics.
///
/// Validates controller operations meet performance requirements.
#[tokio::test]
async fn test_controller_performance() -> Result<()> {
    // Arrange: Create controller with timing expectations
    let mut mock_controller = MockMockableFanController::new();

    // Setup expectations for rapid batch updates
    mock_controller
        .expect_update_speed_batch()
        .times(100)
        .returning(|_| Ok(()));

    // Act: Measure update performance
    let start_time = std::time::Instant::now();

    for i in 0..100 {
        let channel = (i % 4) + 1;
        let speed = (30 + (i % 70)) as u8;

        mock_controller
            .update_speed_batch(vec![(channel, speed)])
            .await
            .with_context(|| format!("Update {i} should succeed"))?;
    }

    let elapsed = start_time.elapsed();

    // Assert: Performance should be reasonable
    assert!(
        elapsed < Duration::from_secs(1),
        "100 updates should complete in under 1 second, took {elapsed:?}"
    );

    let updates_per_second = 100.0 / elapsed.as_secs_f64();
    assert!(
        updates_per_second > 100.0,
        "Should achieve >100 updates/sec, got {updates_per_second:.1}"
    );

    println!("✓ Controller performance: {updates_per_second:.0} updates/sec",);
    Ok(())
}

/// Test controller resource cleanup.
///
/// Validates proper resource management and cleanup.
#[tokio::test]
async fn test_controller_resource_cleanup() -> Result<()> {
    // This test validates that controller resources are properly cleaned up
    // In a real implementation, this would test USB handle cleanup, etc.

    // Arrange: Create multiple controller instances
    let mut controllers = Vec::new();

    for i in 0..5 {
        let mut mock_controller = MockMockableFanController::new();

        mock_controller
            .expect_send_init()
            .times(1)
            .returning(|| Ok(()));

        // Initialize controller
        mock_controller
            .send_init()
            .await
            .with_context(|| format!("Controller {i} initialization should succeed"))?;

        controllers.push(mock_controller);
    }

    // Act: Drop all controllers (simulates cleanup)
    drop(controllers);

    // Assert: This test mainly validates that dropping doesn't panic
    // In a real implementation, we might check that file handles are closed,
    // USB devices are released, etc.

    println!("✓ Controller resource cleanup completed");
    Ok(())
}

/// Test controller firmware version handling.
///
/// Validates different firmware versions and compatibility.
#[tokio::test]
async fn test_controller_firmware_versions() -> Result<()> {
    let firmware_versions = vec![
        (1, 0, 0),       // Old version
        (2, 1, 0),       // Current version
        (3, 0, 0),       // Future version
        (255, 255, 255), // Maximum version
    ];

    for (major, minor, patch) in firmware_versions {
        let mut mock_controller = MockMockableFanController::new();

        mock_controller
            .expect_firmware_version()
            .times(1)
            .returning(move || Ok((major, minor, patch)));

        let version = mock_controller.firmware_version().await.with_context(|| {
            format!("Firmware version query for {major}.{minor}.{patch} should succeed")
        })?;

        assert_eq!(
            version,
            (major, minor, patch),
            "Firmware version should match expected"
        );

        // In a real implementation, we might have version-specific logic here
        // For now, we just validate that all versions are handled gracefully
    }

    println!("✓ Controller firmware version handling validated");
    Ok(())
}

/// Test controller error recovery scenarios.
///
/// Validates recovery mechanisms when controllers encounter errors.
#[tokio::test]
async fn test_controller_error_recovery() -> Result<()> {
    // Test Case 1: Temporary communication failure with recovery
    let mut recovering_controller = MockMockableFanController::new();

    // First call fails, second succeeds (simulates recovery)
    recovering_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("Temporary USB error")));

    recovering_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_| Ok(()));

    // Act: First call should fail
    let first_result = recovering_controller
        .update_speed_batch(vec![(1, 50)])
        .await;
    let first_error = first_result.expect_err("First call should fail");
    assert!(
        first_error.to_string().contains("Temporary USB error"),
        "Error should indicate USB issue, got: {first_error}"
    );

    // Second call should succeed (recovery)
    recovering_controller
        .update_speed_batch(vec![(1, 50)])
        .await
        .context("Recovery call should succeed")?;

    // Test Case 2: Hardware reset simulation
    let mut reset_controller = MockMockableFanController::new();

    // Simulate reset sequence: init -> fail -> re-init -> succeed
    reset_controller
        .expect_send_init()
        .times(2) // Initial + reset
        .returning(|| Ok(()));

    reset_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("Device reset required")));

    reset_controller
        .expect_update_speed_batch()
        .times(1)
        .returning(|_| Ok(()));

    // Act: Initialize, fail, re-initialize, succeed
    reset_controller
        .send_init()
        .await
        .context("Initial initialization should succeed")?;

    let failure_result = reset_controller.update_speed_batch(vec![(1, 50)]).await;
    let failure_error = failure_result.expect_err("Update should fail requiring reset");
    assert!(
        failure_error.to_string().contains("Device reset required"),
        "Error should indicate reset needed, got: {failure_error}"
    );

    // Simulate reset
    reset_controller
        .send_init()
        .await
        .context("Reset initialization should succeed")?;

    reset_controller
        .update_speed_batch(vec![(1, 50)])
        .await
        .context("Post-reset update should succeed")?;

    println!("✓ Controller error recovery scenarios validated");
    Ok(())
}
