//! External interface layer for tt_riingd daemon.
//!
//! This module provides external interfaces for interacting with the tt_riingd daemon
//! from other applications, system services, and command-line tools. It implements
//! standardized APIs and protocols for system integration.
//!
//! # Available Interfaces
//!
//! ## D-Bus Interface ([`dbus_interface`])
//! - **System Integration**: Standard Linux D-Bus interface
//! - **Remote Control**: Fan curve switching and temperature monitoring
//! - **Event Notifications**: Real-time system events and status updates
//! - **Service Discovery**: Standard D-Bus service registration
//!
//! # D-Bus API Overview
//!
//! The D-Bus interface provides a comprehensive API for external control:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    D-Bus Service                            │
//! │                io.github.tt_riingd                          │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Object Path                                 │
//! │              /io/github/tt_riingd                           │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Interface                                 │
//! │              io.github.tt_riingd1                           │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!     ┌────────────────────────┼────────────────────────┐
//!     │                        │                        │
//!     ▼                        ▼                        ▼
//! ┌─────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │   Methods   │    │   Properties    │    │     Signals     │
//! │             │    │                 │    │                 │
//! └─────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! # Usage Examples
//!
//! ## Command Line Integration
//!
//! ```bash
//! # Get active curve for controller
//! busctl --user call io.github.tt_riingd /io/github/tt_riingd \
//!     io.github.tt_riingd1 GetActiveCurve sy "main_controller" 1
//!
//! # Switch to performance curve
//! busctl --user call io.github.tt_riingd /io/github/tt_riingd \
//!     io.github.tt_riingd1 SwitchActiveCurve sys "main_controller" 1 "performance"
//!
//! # Monitor temperature events
//! busctl --user monitor io.github.tt_riingd
//! ```
//!
//! ## Programming Interface (via D-Bus)
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! // D-Bus programming interface example
//! println!("D-Bus methods available for external control");
//! # Ok(())
//! # }
//! ```
//!
//! # API Methods
//!
//! ## Core Control Methods
//! - `GetActiveCurve(controller_id: String, fan_idx: u8) -> String`
//! - `SwitchActiveCurve(controller_id: String, fan_idx: u8, curve_id: String)`
//! - `UpdateCurveData(controller_id: String, fan_idx: u8, curve_id: String, data: String)`
//! - `Stop()` - Gracefully shutdown the daemon
//!
//! ## Information Methods
//! - `GetVersion() -> String` - Get daemon version
//! - `GetControllers() -> Vec<String>` - List available controllers
//! - `GetSensors() -> Vec<String>` - List available sensors
//!
//! # Properties
//! - `Version: String` - Current daemon version
//! - `Status: String` - Daemon operational status
//!
//! # Signals
//! - `Stopped` - Daemon is shutting down
//! - `TemperatureChanged(sensor_id: String, temperature: f64)` - Temperature update
//! - `ConfigReloaded` - Configuration has been reloaded
//! - `DeviceConnected(vendor_id: u16, product_id: u16)` - Hardware connected
//! - `DeviceDisconnected(vendor_id: u16, product_id: u16)` - Hardware disconnected
//!
//! # Error Handling
//!
//! D-Bus methods return standard D-Bus errors:
//! - `org.freedesktop.DBus.Error.InvalidArgs` - Invalid method arguments
//! - `org.freedesktop.DBus.Error.Failed` - Operation failed
//! - `io.github.tt_riingd.Error.ControllerNotFound` - Controller not available
//! - `io.github.tt_riingd.Error.CurveNotFound` - Curve not defined
//!
//! # Security and Permissions
//!
//! - **User Session**: D-Bus service runs in user session bus
//! - **Access Control**: Methods available to session owner only
//! - **Service Activation**: Can be started on-demand via D-Bus activation
//! - **Signal Broadcasting**: Temperature signals broadcast to all subscribers

pub mod dbus_interface;
