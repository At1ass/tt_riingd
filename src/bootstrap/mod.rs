//! Bootstrap and system initialization for tt_riingd daemon.
//!
//! Handles the initialization, configuration, and runtime setup for the
//! tt_riingd daemon. Provides the entry point and foundational components
//! needed to start the system safely and efficiently.
//!
//! Includes CLI argument parsing, logging setup, runtime preparation, daemon
//! initialization, and application startup. Supports both daemon and foreground
//! modes with comprehensive error handling and security considerations.
//!
//! For detailed documentation, startup sequences, and configuration options, see the
//! [Bootstrap Guide](https://docs.tt-riingd.rs/bootstrap/)

pub mod cli;
pub mod daemon;
pub mod logging;
pub mod runtime;
