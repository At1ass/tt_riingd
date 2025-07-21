//! Temperature sensor integration for system monitoring.
//!
//! Unified interface for reading temperature data from various hardware sensors
//! and integrating them with the fan control system. Supports multiple sensor
//! types with automatic discovery and error handling.
//!
//! Supported sensors include lm-sensors (CPU, motherboard), NVIDIA GPU sensors,
//! and dummy sensors for testing. Features robust error handling and graceful
//! degradation when hardware is unavailable.
//!
//! For detailed documentation, configuration examples, and sensor setup, see the
//! [Temperature Sensors Guide](https://docs.tt-riingd.rs/temperature-sensors/)

mod dummy_sensor;
mod lm_sensor;
mod nvidia;
pub mod sensor;
pub mod sensor_manager;
