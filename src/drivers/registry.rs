//! Hardware registry for automatic controller detection and configuration caching.
//!
//! The Registry module provides a centralized system for:
//! - Automatic hardware detection and driver selection
//! - Configuration caching for hotplug support
//! - Fallback controller creation for unknown devices
//! - Static registration of supported hardware types
//!
//! # Architecture
//!
//! The Registry implements a factory pattern with static caching to support
//! the hotplug architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Registry (Static)                       │
//! │  ┌─────────────────────────────────────────────────────────┐ │
//! │  │            Supported Hardware                           │ │
//! │  │  TTRiingQuad, Future drivers...                         │ │
//! │  └─────────────────────────────────────────────────────────┘ │
//! │  ┌─────────────────────────────────────────────────────────┐ │
//! │  │            Configuration Cache                          │ │
//! │  │  Device configs for hotplug restoration                │ │
//! │  └─────────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │               Controller Instances                          │
//! │         Arc<dyn FanController> objects                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Hotplug Configuration Caching
//!
//! The Registry maintains a static cache of controller configurations to support
//! seamless hotplug operations:
//!
//! 1. **Initial Registration**: Configs cached when controllers first created
//! 2. **Device Disconnect**: Controllers destroyed, configs remain cached
//! 3. **Device Reconnect**: New controllers created from cached configs
//! 4. **Fallback Creation**: Unknown devices get default configurations
//!
//! # Examples
//!
//! ## Building Controllers from Configuration
//!
//! ```no_run
//! use tt_riingd::drivers::registry::Registry;
//! use tt_riingd::config::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = Config::default();
//! let controllers = Registry::build_controllers_from_config(&config).await?;
//! println!("Created {} controllers", controllers.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Hardware Fingerprint-based Creation
//!
//! ```no_run
//! use tt_riingd::drivers::{registry::Registry, HardwareFingerprint};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let fingerprint = HardwareFingerprint {
//!     vendor_id: 0x264a,
//!     product_id: 0x2330,
//!     serial: Some("ABC123".to_string()),
//! };
//!
//! // Attempts to restore from cache, creates fallback if not found
//! let controller = Registry::build_controller_from_fingerprint(&fingerprint).await?;
//! controller.send_init().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Automatic Hardware Detection
//!
//! ```no_run
//! use tt_riingd::drivers::{registry::Registry, HardwareFingerprint};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create controller from hardware fingerprint (for hotplug restoration)
//! let fingerprint = HardwareFingerprint {
//!     vendor_id: 0x264a,
//!     product_id: 0x2330,
//!     serial: Some("ABC123".to_string()),
//! };
//!
//! // Build controller from cached configuration or create fallback
//! let controller = Registry::build_controller_from_fingerprint(&fingerprint).await?;
//! let id = controller.get_id().await;
//! println!("Created controller: {}", id);
//! # Ok(())
//! # }
//! ```

use super::{HIDAPI, HardwareFingerprint};
use crate::config::{
    Config,
    cfg::{ControllerCfg, UsbSelector},
};
use crate::drivers::fan_controller::FanController;
use crate::drivers::tt_riing_quad::TTRiingQuad;
use anyhow::Result;
use dashmap::DashMap;
use futures::{StreamExt, stream};
use std::sync::{Arc, LazyLock};
use tracing::{debug, info, warn};

/// Static hardware information for controller driver registration.
///
/// Contains metadata about supported hardware types including USB identifiers,
/// capabilities, and factory functions for creating fallback configurations.
/// Each hardware driver must provide this information for Registry registration.
///
/// # Fields
///
/// - `vid`: USB Vendor ID (typically 0x264a for Thermaltake)
/// - `pids`: List of supported USB Product IDs for this hardware type
/// - `channel_count`: Maximum number of fan channels supported
/// - `name`: Human-readable hardware name for logging and identification
/// - `create_fallback_config`: Factory function for creating default configurations
///
/// # Examples
///
/// ```no_run
/// use tt_riingd::drivers::{registry::HardwareInfo, HardwareFingerprint};
/// use tt_riingd::config::{ControllerCfg, UsbSelector};
///
/// let info = HardwareInfo {
///     vid: 0x264a,
///     pids: vec![0x2330, 0x2331],
///     channel_count: 5,
///     name: "TTRiingQuad".to_string(),
///     create_fallback_config: |fingerprint| {
///         ControllerCfg::RiingQuad {
///             id: "fallback".to_string(),
///             usb: UsbSelector {
///                 vid: fingerprint.vendor_id,
///                 pid: fingerprint.product_id,
///                 serial: fingerprint.serial.clone(),
///             },
///             fans: vec![],
///         }
///     },
/// };
/// ```
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    /// USB Vendor ID for this hardware type.
    pub vid: u16,
    /// List of supported USB Product IDs.
    pub pids: Vec<u16>,
    /// Maximum number of fan channels this hardware supports.
    pub channel_count: u8,
    /// Human-readable name for this hardware type.
    pub name: String,
    /// Factory function for creating fallback configurations.
    ///
    /// Called when a device with matching fingerprint is detected but no
    /// cached configuration exists. Should create a minimal working config.
    pub create_fallback_config: fn(&HardwareFingerprint) -> ControllerCfg,
}

/// Controller hardware detected during system scan.
///
/// Internal structure used during hardware enumeration to track
/// detected devices before controller instantiation.
#[derive(Debug)]
struct DetectedController {
    /// Hardware information for the detected device type.
    hardware_info: HardwareInfo,
    /// Specific Product ID detected on the system.
    detected_pid: u16,
}

impl DetectedController {
    /// Creates a new detected controller entry.
    ///
    /// # Arguments
    ///
    /// * `hardware_info` - Static hardware information for this device type
    /// * `detected_pid` - Specific PID found during system scan
    fn new(hardware_info: HardwareInfo, detected_pid: u16) -> Self {
        Self {
            hardware_info,
            detected_pid,
        }
    }
}

/// Central registry for hardware detection and controller management.
///
/// Provides static methods for:
/// - Hardware detection and driver instantiation
/// - Configuration caching for hotplug support  
/// - Fallback controller creation
/// - Supported hardware registration
///
/// The Registry implements a factory pattern with caching to enable
/// seamless hotplug operations while maintaining performance.
pub struct Registry;

impl Registry {
    /// Get list of all supported hardware (cached)
    fn get_supported_hardware() -> &'static [HardwareInfo] {
        static SUPPORTED_HARDWARE: LazyLock<Vec<HardwareInfo>> = LazyLock::new(|| {
            vec![
                TTRiingQuad::hardware_info(),
                // Add more controller types here in the future
            ]
        });
        &SUPPORTED_HARDWARE
    }

    fn get_configurarion_cache() -> &'static DashMap<HardwareFingerprint, ControllerCfg> {
        static CONFIG_CACHE: LazyLock<DashMap<HardwareFingerprint, ControllerCfg>> =
            LazyLock::new(DashMap::new);
        &CONFIG_CACHE
    }

    pub fn is_supported_hardware(vid: u16, pid: u16) -> bool {
        Self::get_supported_hardware()
            .iter()
            .any(|hw| hw.vid == vid && hw.pids.contains(&pid))
    }

    async fn build_one_controller(
        controller_cfg: &ControllerCfg,
    ) -> Result<Arc<dyn FanController>> {
        debug!(
            "Building controller from configuration: {:?}",
            controller_cfg
        );

        let hidapi = HIDAPI
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HID API not available"))?;

        match controller_cfg {
            ControllerCfg::RiingQuad { id, usb, fans: _ } => {
                info!(
                    "Initializing TTRiingQuad controller: {} (VID:PID {:04X}:{:04X})",
                    id, usb.vid, usb.pid
                );
                TTRiingQuad::create_one(hidapi, controller_cfg)
            } // Add more controller types here in the future
        }
    }

    pub async fn build_controllers_from_config(
        config: &Config,
    ) -> Result<Vec<Arc<dyn FanController>>> {
        debug!("Building controllers from configuration");

        let cacher = Self::get_configurarion_cache();

        let controllers = stream::iter(config.controllers.iter())
            .filter_map(|controller_cfg| async move {
                let created = Self::build_one_controller(controller_cfg).await;
                if let Ok(controller) = created {
                    if let Ok(fingerprint) = controller.get_fingerprint().await {
                        cacher.insert(fingerprint, controller_cfg.clone());
                    } else {
                        warn!(
                            "Failed to get fingerprint for controller: {:?}",
                            controller_cfg
                        );
                    }
                    Some(controller)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .await;

        info!("Built {} controllers from configuration", controllers.len());
        Ok(controllers)
    }

    pub async fn build_controller_from_fingerprint(
        fingerprint: &HardwareFingerprint,
    ) -> Result<Arc<dyn FanController>> {
        debug!("Building controller from fingerprint: {:?}", fingerprint);

        let cacher = Self::get_configurarion_cache();

        if let Some(config) = cacher.get(fingerprint) {
            debug!(
                "Found cached configuration for fingerprint: {:?}",
                fingerprint
            );
            Self::build_one_controller(&config).await
        } else {
            debug!(
                "No cached configuration found for fingerprint: {:?}, building default",
                fingerprint
            );
            Self::build_default_controller(fingerprint).await
        }
    }

    async fn build_default_controller(
        fingerprint: &HardwareFingerprint,
    ) -> Result<Arc<dyn FanController>> {
        let fallback_config = (TTRiingQuad::hardware_info().create_fallback_config)(fingerprint);
        let cacher = Self::get_configurarion_cache();

        let controller = Self::build_one_controller(&fallback_config).await;

        if let Ok(ctrl) = controller {
            cacher.insert(fingerprint.clone(), fallback_config);
            Ok(ctrl)
        } else {
            Err(anyhow::anyhow!(
                "Failed to build default controller for fingerprint: {:?}",
                fingerprint
            ))
        }
    }

    /// Scan hardware and merge with existing config
    pub fn merge_with_config(config: &mut Config) {
        debug!("Starting hardware scan and config merge");

        let detected = Self::scan_hardware();

        let added_count = detected.iter().fold(0, |count, detected_controller| {
            if !Self::controller_exists_in_config(&config.controllers, detected_controller) {
                // Add new controller with fallback settings
                let fallback_config = (detected_controller.hardware_info.create_fallback_config)(&HardwareFingerprint {
                    vendor_id: detected_controller.hardware_info.vid,
                    product_id: detected_controller.detected_pid,
                    serial: None, // Serial is not used in fallback
                });
                warn!(
                    "Controller {}:{} (VID:PID {:04X}:{:04X}) not found in configuration, adding with fallback settings",
                    detected_controller.hardware_info.name,
                    Self::get_controller_id(&fallback_config),
                    detected_controller.hardware_info.vid,
                    detected_controller.detected_pid
                );
                config.controllers.push(fallback_config);
                count + 1
            } else {
                count
            }
        });

        if added_count > 0 {
            info!("Added {} controllers to configuration", added_count);
        }
    }

    /// Validate that all controllers in config are supported
    pub fn validate_supported_controllers(config: &Config) -> Vec<String> {
        debug!("Validating controller support in configuration");

        let supported_hardware = Self::get_supported_hardware();

        let unsupported = config
            .controllers
            .iter()
            .filter_map(|cfg| {
                if let Some(usb) = Self::extract_usb_info(cfg) {
                    if !supported_hardware
                        .iter()
                        .any(|hw| hw.vid == usb.vid && hw.pids.contains(&usb.pid))
                    {
                        Some(format!(
                            "Controller '{}' (VID:PID {:04X}:{:04X}) is not supported",
                            Self::get_controller_id(cfg),
                            usb.vid,
                            usb.pid
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if unsupported.is_empty() {
            info!(
                "All {} controllers in configuration are supported",
                config.controllers.len()
            );
        } else {
            warn!(
                "Found {} unsupported controllers in configuration",
                unsupported.len()
            );
        }

        unsupported
    }

    /// Scan for all supported hardware controllers
    fn scan_hardware() -> Vec<DetectedController> {
        debug!("Starting hardware scan for supported controllers");

        let hidapi = match HIDAPI.as_ref() {
            Some(api) => api,
            None => {
                warn!("HID API not available, skipping hardware scan");
                return Vec::new();
            }
        };

        let device_list = hidapi.device_list();
        let supported_hardware = Self::get_supported_hardware();

        let detected: Vec<DetectedController> = device_list
            .filter_map(|device| {
                supported_hardware.iter().find_map(|hw_info| {
                    if device.vendor_id() == hw_info.vid
                        && hw_info.pids.contains(&device.product_id())
                    {
                        info!(
                            "Detected controller: {} (VID:PID {:04X}:{:04X})",
                            hw_info.name,
                            device.vendor_id(),
                            device.product_id()
                        );
                        Some(DetectedController::new(
                            hw_info.clone(),
                            device.product_id(),
                        ))
                    } else {
                        None
                    }
                })
            })
            .collect();

        info!(
            "Hardware scan completed. Found {} controllers",
            detected.len()
        );
        detected
    }

    /// Check if controller already exists in configuration
    fn controller_exists_in_config(
        controllers: &[ControllerCfg],
        detected: &DetectedController,
    ) -> bool {
        controllers.iter().any(|cfg| {
            Self::extract_usb_info(cfg)
                .map(|usb| {
                    usb.vid == detected.hardware_info.vid && usb.pid == detected.detected_pid
                })
                .unwrap_or(false)
        })
    }

    /// Extract USB information from controller configuration
    fn extract_usb_info(config: &ControllerCfg) -> Option<&UsbSelector> {
        match config {
            ControllerCfg::RiingQuad { usb, .. } => Some(usb),
        }
    }

    /// Get controller ID from configuration
    fn get_controller_id(config: &ControllerCfg) -> &str {
        match config {
            ControllerCfg::RiingQuad { id, .. } => id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detected_controller_fallback_config() {
        let hw_info = HardwareInfo {
            vid: 0x264A,
            pids: vec![0x232B, 0x232C],
            channel_count: 4,
            name: "Test Controller".to_string(),
            create_fallback_config: |_fingerprint| ControllerCfg::RiingQuad {
                id: "test".to_string(),
                usb: UsbSelector {
                    vid: 0x264A,
                    pid: 0x232B,
                    serial: None,
                },
                fans: vec![],
            },
        };

        let detected = DetectedController::new(hw_info, 0x232B);
        let fingerprint = HardwareFingerprint {
            vendor_id: 0x264A,
            product_id: 0x232B,
            serial: None,
        };
        let config = (detected.hardware_info.create_fallback_config)(&fingerprint);

        assert_eq!(detected.detected_pid, 0x232B);

        match config {
            ControllerCfg::RiingQuad { id, usb, fans: _ } => {
                assert_eq!(id, "test");
                assert_eq!(usb.vid, 0x264A);
                assert_eq!(usb.pid, 0x232B);
            }
        }
    }

    #[test]
    fn test_controller_exists_in_config() {
        let hw_info = HardwareInfo {
            vid: 0x264A,
            pids: vec![0x232B],
            channel_count: 4,
            name: "Test Controller".to_string(),
            create_fallback_config: |_fingerprint| ControllerCfg::RiingQuad {
                id: "test".to_string(),
                usb: UsbSelector {
                    vid: 0x264A,
                    pid: 0x232B,
                    serial: None,
                },
                fans: vec![],
            },
        };

        let detected = DetectedController::new(hw_info, 0x232B);

        let existing_config = vec![ControllerCfg::RiingQuad {
            id: "existing-controller".to_string(),
            usb: UsbSelector {
                vid: 0x264A,
                pid: 0x232B,
                serial: None,
            },
            fans: vec![],
        }];

        assert!(Registry::controller_exists_in_config(
            &existing_config,
            &detected
        ));

        let different_config = vec![ControllerCfg::RiingQuad {
            id: "different-controller".to_string(),
            usb: UsbSelector {
                vid: 0x264A,
                pid: 0x232C, // Different PID
                serial: None,
            },
            fans: vec![],
        }];

        assert!(!Registry::controller_exists_in_config(
            &different_config,
            &detected
        ));
    }

    #[test]
    fn test_validate_supported_controllers() {
        use crate::config::Config;

        // Test with supported controller
        let supported_config = Config {
            controllers: vec![ControllerCfg::RiingQuad {
                id: "supported-controller".to_string(),
                usb: UsbSelector {
                    vid: 0x264a, // TTRiingQuad VID
                    pid: 0x232B, // TTRiingQuad supported PID
                    serial: None,
                },
                fans: vec![],
            }],
            ..Default::default()
        };

        let errors = Registry::validate_supported_controllers(&supported_config);
        assert!(errors.is_empty());

        // Test with unsupported controller
        let unsupported_config = Config {
            controllers: vec![ControllerCfg::RiingQuad {
                id: "unsupported-controller".to_string(),
                usb: UsbSelector {
                    vid: 0x1234, // Unknown VID
                    pid: 0x5678, // Unknown PID
                    serial: None,
                },
                fans: vec![],
            }],
            ..Default::default()
        };

        let errors = Registry::validate_supported_controllers(&unsupported_config);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unsupported-controller"));
        assert!(errors[0].contains("1234:5678"));
    }
}
