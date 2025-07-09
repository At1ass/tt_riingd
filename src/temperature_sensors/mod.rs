//! Temperature sensor integration for system monitoring.
//!
//! This module provides a unified interface for reading temperature data from various
//! hardware sensors and integrating them with the fan control system. It supports
//! multiple sensor types with automatic discovery and error handling.
//!
//! # Supported Sensor Types
//!
//! ## lm-sensors Integration
//! - **CPU Temperature**: AMD and Intel processor thermal sensors
//! - **Motherboard Sensors**: System temperature monitoring
//! - **Custom Sensors**: Any sensor exposed through lm-sensors
//!
//! ## NVIDIA GPU Integration
//! - **GPU Temperature**: NVIDIA graphics card thermal monitoring
//! - **Multi-GPU Support**: Multiple graphics cards with individual monitoring
//! - **Driver Integration**: Direct NVML (NVIDIA Management Library) access
//!
//! ## Testing and Development
//! - **Dummy Sensors**: Configurable test sensors for development
//! - **Mock Implementation**: Unit testing support
//!
//! # Architecture Overview
//!
//! The sensor system follows a provider pattern with centralized management:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   SensorManager                             │
//! │          Registration, discovery, polling coordination      │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  Sensor Trait                               │
//! │         Unified interface for all sensor types             │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!     ┌────────────────────────┼────────────────────────┐
//!     │                        │                        │
//!     ▼                        ▼                        ▼
//! ┌─────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ lm-sensors  │    │ NVIDIA GPU      │    │ Dummy/Mock      │
//! │ Integration │    │ Integration     │    │   Sensors       │
//! └─────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! # Sensor Discovery and Registration
//!
//! Sensors are automatically discovered and registered during system initialization:
//!
//! 1. **Configuration Parsing**: Sensor definitions loaded from YAML config
//! 2. **Hardware Detection**: Available sensors detected on system
//! 3. **Driver Loading**: Appropriate sensor drivers instantiated
//! 4. **Registration**: Sensors registered with SensorManager
//! 5. **Polling Setup**: Periodic temperature reading configured
//!
//! # Error Handling and Resilience
//!
//! The sensor system implements robust error handling:
//! - **Graceful Degradation**: Failed sensors don't affect other sensors
//! - **Retry Logic**: Temporary failures handled with exponential backoff
//! - **Fallback Sensors**: Dummy sensors used when hardware unavailable
//! - **Error Reporting**: Sensor failures logged and reported via events
//!
//! # Examples
//!
//! ## Basic Sensor Usage
//!
//! ```no_run
//! use tt_riingd::temperature_sensors::sensor_manager::SensorManager;
//! use tt_riingd::config::{Config, SensorCfg};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = Config::default();
//! let sensor_manager = SensorManager::init_from_cfg(&config)?;
//!
//! // Access sensors through the manager
//! println!("Sensor manager initialized");
//! # Ok(())
//! # }
//! ```
//!
//! ## NVIDIA GPU Temperature Monitoring
//!
//! ```no_run
//! use tt_riingd::temperature_sensors::sensor_manager::SensorManager;
//! use tt_riingd::config::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = Config::default();
//! let sensor_manager = SensorManager::init_from_cfg(&config)?;
//!
//! // Monitor sensors through the manager
//! println!("GPU sensor manager ready");
//! # Ok(())
//! # }
//! ```
//!
//! ## Multi-Sensor Monitoring
//!
//! ```no_run
//! use tt_riingd::temperature_sensors::sensor_manager::SensorManager;
//! use tt_riingd::config::Config;
//!
//! # async fn example(config: &Config) -> anyhow::Result<()> {
//! let sensor_manager = SensorManager::init_from_cfg(config)?;
//!
//! // All sensors from configuration are automatically registered
//! println!("All sensors initialized from configuration");
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration Examples
//!
//! ## lm-sensors Configuration
//!
//! ```yaml
//! sensors:
//!   - kind: lm-sensors
//!     id: "cpu_temp"
//!     chip: "k10temp-pci-00c3"    # AMD Ryzen CPU
//!     feature: "Tctl"
//!   
//!   - kind: lm-sensors
//!     id: "motherboard_temp"
//!     chip: "nct6798-isa-0290"    # Motherboard sensor
//!     feature: "temp1"
//! ```
//!
//! ## NVIDIA GPU Configuration
//!
//! ```yaml
//! sensors:
//!   - kind: nvidia
//!     id: "gpu0_temp"
//!     gpu_index: 0               # First GPU
//!   
//!   - kind: nvidia
//!     id: "gpu1_temp"
//!     gpu_index: 1               # Second GPU (multi-GPU setup)
//! ```
//!
//! # Platform Support
//!
//! - **Linux**: Full support for lm-sensors and NVIDIA drivers
//! - **Other Platforms**: Limited support, dummy sensors for testing
//!
//! # Dependencies
//!
//! - **lm-sensors**: System package required for hardware sensor access
//! - **NVIDIA Drivers**: Proprietary drivers required for GPU monitoring
//! - **Permissions**: May require udev rules for non-root sensor access

mod dummy_sensor;
mod lm_sensor;
mod nvidia;
pub mod sensor;
pub mod sensor_manager;
