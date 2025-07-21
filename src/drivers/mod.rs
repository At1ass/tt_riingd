//! Hardware abstraction layer for fan controller drivers.
//!
//! Provides modular driver architecture with hotplug detection, hardware fingerprinting,
//! and unified control interfaces for Thermaltake Riing fans and compatible hardware.
//!
//! See the [Hardware Abstraction Guide](https://docs.rs/tt_riingd/latest/tt_riingd/docs/hardware.html)
//! for detailed architecture, driver implementation, and batch operations.

use std::sync::LazyLock;

use hidapi::HidApi;
use tracing::{info, warn};

pub mod commands;
pub mod controller_manager;
pub mod fan_controller;
pub mod registry;
pub mod tt_riing_quad;
mod worker_manager;

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
