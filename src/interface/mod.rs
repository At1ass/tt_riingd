//! External interface layer for tt_riingd daemon.
//!
//! Provides external interfaces for interacting with the tt_riingd daemon from
//! other applications, system services, and command-line tools. Implements
//! standardized APIs and protocols for system integration.
//!
//! Features D-Bus interface for system integration, remote control, event
//! notifications, and service discovery. Supports fan curve switching,
//! temperature monitoring, and real-time system events.
//!
//! For detailed documentation, API reference, and usage examples, see the
//! [External Interfaces Guide](https://docs.tt-riingd.rs/interfaces/)

pub mod dbus_interface;
