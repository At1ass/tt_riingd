//! Hardware abstraction layer for fan controller drivers.
//!
//! This module provides the hardware abstraction layer for controlling Thermaltake Riing fans
//! and other compatible hardware. It implements a modular driver architecture that supports
//! hotplug detection, hardware fingerprinting, and unified control interfaces.
//!
//! # Architecture Overview
//!
//! The drivers module is organized around these key concepts:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ControllerManager                        │
//! │          High-level controller lifecycle management        │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   WorkerManager                             │
//! │          Async worker threads for hardware I/O             │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     Registry                                │
//! │       Hardware detection and configuration caching         │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  FanController Trait                       │
//! │         Unified interface for all hardware types           │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                Hardware Implementations                     │
//! │    TTRiingQuad, MockController, Future drivers...          │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Hardware Fingerprinting
//!
//! Each hardware device is identified by a [`HardwareFingerprint`] consisting of:
//! - **Vendor ID** (VID): USB vendor identifier
//! - **Product ID** (PID): USB product identifier  
//! - **Serial Number**: Optional unique device serial
//!
//! This enables stable identification across device reconnections and supports
//! configuration persistence during hotplug events.
//!
//! # Driver Registration
//!
//! Hardware drivers are registered with the [`Registry`](registry::Registry) using static hardware information:
//!
//! ```no_run
//! use tt_riingd::drivers::registry::Registry;
//! use tt_riingd::config::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Registry automatically detects and instantiates controllers
//! let config = Config::default();
//! let controllers = Registry::build_controllers_from_config(&config).await?;
//! println!("Created {} controllers", controllers.len());
//! # Ok(())
//! # }
//! ```
//!
//! # Batch Operations
//!
//! The driver system supports efficient batch operations for performance:
//!
//! ```no_run
//! use tt_riingd::drivers::commands::{BatchCommand, ExecutionMode};
//! use std::collections::HashMap;
//!
//! # async fn example(controller_manager: &tt_riingd::drivers::controller_manager::ControllerManager) -> anyhow::Result<()> {
//! let mut speed_data = HashMap::new();
//! speed_data.insert("main_controller".to_string(), vec![(1, 75), (2, 80)]);
//!
//! let result = controller_manager.batch_update(
//!     BatchCommand::SetSpeeds { data: &speed_data },
//!     ExecutionMode::Blocking
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Hotplug Support
//!
//! The system automatically handles device connect/disconnect events:
//!
//! 1. **Device Detection**: USB events detected via udev integration
//! 2. **Fingerprint Matching**: Hardware identified by VID/PID/serial
//! 3. **Configuration Restoration**: Cached configs applied to reconnected devices
//! 4. **Worker Management**: Background threads started/stopped dynamically
//!
//! # Error Handling
//!
//! All driver operations return `anyhow::Result<T>` for consistent error propagation.
//! Hardware failures are handled gracefully with automatic retry logic and fallback
//! to safe-mode operation when controllers become unavailable.
//!
//! # Examples
//!
//! ## Basic Controller Usage
//!
//! ```no_run
//! use tt_riingd::drivers::{fan_controller::FanController, tt_riing_quad::TTRiingQuad};
//! use tt_riingd::config::{ControllerCfg, UsbSelector, FanCfg};
//! use hidapi::HidApi;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let api = HidApi::new()?;
//! let config = vec![ControllerCfg::RiingQuad {
//!     id: "main".to_string(),
//!     usb: UsbSelector { vid: 0x264a, pid: 0x2330, serial: None },
//!     fans: vec![FanCfg { idx: 1, name: "CPU Fan".to_string() }],
//! }];
//!
//! let controllers = TTRiingQuad::find_controllers(&api, &config)?;
//! for controller in controllers {
//!     controller.send_init().await?;
//!     controller.update_speed_batch(&[(1, 60)]).await?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Hardware Fingerprinting
//!
//! ```no_run
//! use tt_riingd::drivers::HardwareFingerprint;
//!
//! let fingerprint = HardwareFingerprint {
//!     vendor_id: 0x264a,    // Thermaltake VID
//!     product_id: 0x2330,   // Riing Quad PID
//!     serial: Some("ABC123".to_string()),
//! };
//!
//! // Used for stable device identification across reconnections
//! println!("Device: {:?}", fingerprint);
//! ```

use std::sync::LazyLock;

use hidapi::HidApi;
use tracing::{info, warn};

pub mod commands;
pub mod controller_manager;
pub mod fan_controller;
pub mod registry;
pub mod tt_riing_quad;
mod worker_manager;

/// Color data for a single fan channel: (channel_index, RGB_colors).
///
/// Used for sending RGB lighting data to individual fan channels.
/// Each RGB color is represented as a tuple of (red, green, blue) values (0-255).
type ColorBuffer = (usize, Vec<(u8, u8, u8)>);

/// Collection of color data for multiple channels on a single controller.
///
/// Enables batch RGB updates across multiple fan channels simultaneously
/// for improved performance and synchronization.
type ControllerColorBuffer = Vec<ColorBuffer>;

/// Reference version of ColorBuffer for efficient batch operations.
///
/// Uses borrowed slices to avoid copying color data during batch processing.
type ColorBufferForSend<'a> = (usize, &'a [(u8, u8, u8)]);

/// Reference version of ControllerColorBuffer for efficient batch operations.
///
/// Used internally by the worker system to minimize memory allocations
/// during high-frequency RGB updates.
type ControllerColorBufferForSend<'a> = Vec<ColorBufferForSend<'a>>;

/// Global HID API instance for hardware communication.
///
/// Initialized lazily on first access. If HID API initialization fails,
/// hardware control will be disabled but the daemon will continue running
/// in a degraded mode for monitoring and configuration validation.
static HIDAPI: LazyLock<Option<HidApi>> = LazyLock::new(|| match HidApi::new() {
    std::result::Result::Ok(api) => {
        info!("HID API initialized successfully");
        Some(api)
    }
    std::result::Result::Err(e) => {
        warn!(
            "HID API unavailable: {}. Hardware control will be disabled.",
            e
        );
        None
    }
});

/// Hardware device fingerprint for stable identification across reconnections.
///
/// Provides a stable way to identify hardware devices that persists across
/// USB disconnect/reconnect cycles. Used by the hotplug system to restore
/// configurations when devices reconnect.
///
/// # Examples
///
/// ```
/// use tt_riingd::drivers::HardwareFingerprint;
///
/// let fingerprint = HardwareFingerprint {
///     vendor_id: 0x264a,                    // Thermaltake USB VID
///     product_id: 0x2330,                   // Riing Quad USB PID
///     serial: Some("TT-RQ-001".to_string()), // Optional unique serial
/// };
///
/// // Fingerprints can be compared for equality
/// assert_eq!(fingerprint.vendor_id, 0x264a);
/// ```
///
/// # Hotplug Workflow
///
/// 1. Device connects → System reads VID/PID/serial
/// 2. Creates fingerprint → Looks up cached configuration
/// 3. Restores settings → Continues operation seamlessly
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HardwareFingerprint {
    /// USB Vendor ID (16-bit identifier assigned by USB-IF).
    pub vendor_id: u16,
    /// USB Product ID (16-bit identifier assigned by vendor).
    pub product_id: u16,
    /// Optional device serial number for unique identification.
    ///
    /// When present, enables disambiguation between multiple devices
    /// of the same model. When None, identification relies on VID/PID only.
    pub serial: Option<String>,
}
