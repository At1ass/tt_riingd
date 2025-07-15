//! Configuration system for tt_riingd daemon.
//!
//! This module provides a comprehensive configuration system with:
//! - YAML-based configuration files with validation
//! - Hot-reload capability with file watching
//! - Strong typing and semantic validation
//! - Fan curve definitions and temperature mappings
//! - Hardware controller and sensor configuration
//!
//! # Architecture Overview
//!
//! The configuration system is built around these core concepts:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ConfigManager                            │
//! │  File loading, validation, hot-reload coordination         │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                       Config                                │
//! │       Main configuration structure with validation         │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!     ┌────────────────────────┼────────────────────────┐
//!     │                        │                        │
//!     ▼                        ▼                        ▼
//! ┌─────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │   Hardware  │    │   Fan Curves    │    │    Mappings     │
//! │   Config    │    │   & Sensors     │    │   & Effects     │
//! └─────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! # Configuration Structure
//!
//! The main configuration file (`~/.config/tt_riingd/config.yml`) contains:
//!
//! ## Hardware Configuration
//! - **Controllers**: USB device specifications and fan channel definitions
//! - **Sensors**: Temperature sensor configurations (lm-sensors, NVIDIA GPU)
//! - **Curves**: Fan speed curves (constant, step-based, Bézier)
//! - **Colors**: RGB color definitions for lighting effects
//!
//! ## Behavior Configuration  
//! - **Mappings**: Sensor-to-fan assignments
//! - **Active Curves**: Which curves are active for which fans
//! - **Color Mappings**: Temperature-based RGB lighting
//! - **Timing**: Polling intervals and broadcast settings
//!
//! # Hot Reload Support
//!
//! The configuration system supports hot-reload via file watching:
//!
//! 1. **File Monitoring**: ConfigWatcher service monitors config file
//! 2. **Change Detection**: File system events trigger reload process
//! 3. **Validation**: New configuration validated before applying
//! 4. **Event Publishing**: `ConfigReloaded` event notifies all services
//! 5. **Graceful Application**: Services update settings without restart
//!
//! # Examples
//!
//! ## Basic Configuration Loading
//!
//! ```no_run
//! use tt_riingd::config::ConfigManager;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Load configuration from default location
//! let config_manager = ConfigManager::load(None).await?;
//!
//! // Access the configuration
//! let config = config_manager.clone_config().await;
//! println!("Loaded {} controllers", config.controllers.len());
//! println!("Polling interval: {} seconds", config.tick_seconds);
//! # Ok(())
//! # }
//! ```
//!
//! ## Controller Configuration
//!
//! ```yaml
//! controllers:
//!   - kind: riing-quad
//!     id: "main_controller"
//!     usb:
//!       vid: 0x264a
//!       pid: 0x2330
//!       serial: "ABC123"  # Optional
//!     fans:
//!       - idx: 1
//!         name: "CPU Intake"
//!       - idx: 2
//!         name: "CPU Exhaust"
//! ```
//!
//! ## Fan Curve Definitions
//!
//! ```yaml
//! curves:
//!   # Constant speed curve
//!   - kind: constant
//!     id: "silent"
//!     speed: 30
//!
//!   # Step-based curve
//!   - kind: step-curve
//!     id: "performance"
//!     tmps: [30.0, 50.0, 70.0, 85.0]
//!     spds: [25, 40, 70, 100]
//!
//!   # Smooth Bézier curve
//!   - kind: bezier
//!     id: "smooth"
//!     points:
//!       - {x: 30.0, y: 20.0}
//!       - {x: 45.0, y: 30.0}
//!       - {x: 65.0, y: 70.0}
//!       - {x: 80.0, y: 95.0}
//! ```
//!
//! ## Sensor and Mapping Configuration
//!
//! ```yaml
//! sensors:
//!   - kind: lm-sensors
//!     id: "cpu_temp"
//!     chip: "k10temp-pci-00c3"
//!     feature: "Tctl"
//!   
//!   - kind: nvidia
//!     id: "gpu_temp"
//!     gpu_index: 0
//!
//! mappings:
//!   - sensor: "cpu_temp"
//!     targets:
//!       - controller_id: "main_controller"
//!         fan_idx: 1
//!       - controller_id: "main_controller"
//!         fan_idx: 2
//!
//! active_curve_mappings:
//!   - curve: "performance"
//!     targets:
//!       - controller_id: "main_controller"
//!         fan_idx: 1
//! ```
//!
//! ## Configuration Validation
//!
//! ```no_run
//! use tt_riingd::config::ConfigManager;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Load and validate configuration
//! let config_manager = ConfigManager::load(None).await?;
//!
//! // Configuration is automatically validated during loading
//! // Check for configuration changes that require reload vs restart
//! let change_type = config_manager.analyze_config_changes().await?;
//! println!("Configuration change analysis: {:?}", change_type);
//! # Ok(())
//! # }
//! ```

pub mod cfg;
pub mod fan_curve;
pub mod mappings;

#[cfg(test)]
pub mod tests;

pub use crate::core::event::ConfigChangeType;
pub use cfg::{
    Config, ConfigManager, ControllerCfg, CurveCfg, CurveMappingCfg, EffectCfg, EffectMappingCfg,
    FanCfg, FanTarget, MappingCfg, SensorCfg, UsbSelector,
};
pub use fan_curve::{FanCurve, Point};
pub use mappings::{CurveMapping, EffectMapping, EffectStore, FanRef, Mapping};
