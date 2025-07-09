//! RGB lighting effects and visual control system.
//!
//! This module provides a stream-based RGB lighting effect system for fan controllers with LED support.
//! Effects are implemented as async streams that generate RGB color values over time.
//!
//! # Effect System Overview
//!
//! The effects system is built around these core concepts:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  EffectRunner                               │
//! │        Stream-based RGB color generation                   │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                RGB Color Stream                             │
//! │      Async stream yielding [u8; 3] RGB values              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                Fan RGB Controllers                          │
//! │         Hardware that applies colors to LEDs               │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Available Effects
//!
//! ## Static Effects
//! - **ConstantColor**: Single static RGB color
//!
//! ## Dynamic Effects  
//! - **Rainbow**: Continuous hue rotation across the color spectrum
//! - **Breathing**: Smooth brightness fade in/out with configurable period
//! - **Fade**: Color fade effect with specified duration
//!
//! The system supports these effects through the [`EffectCfg`](crate::config::EffectCfg) enum which
//! defines the available effect types and their parameters.
//!
//! # Configuration
//!
//! Effects are configured in the main configuration file:
//!
//! ```yaml
//! effects:
//!   # Static color effect
//!   - kind: constant-color
//!     id: "blue_static"
//!     rgb: [0, 100, 255]
//!   
//!   # Breathing effect
//!   - kind: breathing
//!     id: "blue_breathing"
//!     color: [0, 100, 255]
//!     duration: 3
//!   
//!   # Rainbow effect
//!   - kind: rainbow
//!     id: "rainbow_cycle"
//!     duration: 10
//!   
//!   # Fade effect
//!   - kind: fade
//!     id: "red_fade"
//!     color: [255, 0, 0]
//!     duration: 2
//!
//! effect_mappings:
//!   - effect: "rainbow_cycle"
//!     targets:
//!       - controller_id: "main_controller"
//!         fan_idx: 1
//! ```
//!
//! # Usage Examples
//!
//! ## Creating Effect Runners
//!
//! ```no_run
//! use tt_riingd::effects::effect_runner::EffectRunner;
//! use tokio::time::Duration;
//! use async_stream::stream;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create a constant color effect
//! let blue_static = EffectRunner::constant([0, 100, 255]);
//! 
//! // Create a rainbow effect with 10-second period
//! let rainbow = EffectRunner::rainbow(Duration::from_secs(10));
//! 
//! // Create a breathing effect
//! let breathing = EffectRunner::breathe(
//!     [255, 0, 0],         // Red color
//!     0.1,                 // Minimum brightness
//!     1.0,                 // Maximum brightness  
//!     Duration::from_secs(3) // 3-second period
//! );
//!
//! // Get the next RGB color from any effect
//! if let Some(rgb) = blue_static.next_rgb().await {
//!     println!("RGB: [{}, {}, {}]", rgb[0], rgb[1], rgb[2]);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom Stream-Based Effects
//!
//! ```no_run
//! use tt_riingd::effects::effect_runner::EffectRunner;
//! use async_stream::stream;
//! use tokio::time::{sleep, Duration};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create custom effect that alternates between red and blue
//! let custom_stream = stream! {
//!     let colors = [[255, 0, 0], [0, 0, 255]]; // Red and blue
//!     let mut index = 0;
//!     loop {
//!         yield colors[index];
//!         index = (index + 1) % colors.len();
//!         sleep(Duration::from_millis(500)).await;
//!     }
//! };
//!
//! let custom_effect = EffectRunner::new(custom_stream);
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Considerations
//!
//! - **Update Frequency**: Effects run at configurable frame rates (typically 30-60 FPS)
//! - **Batch Updates**: Multiple LED changes batched for efficiency  
//! - **Memory Usage**: Effect data cached to minimize allocations
//! - **CPU Usage**: Effect calculations optimized for minimal CPU overhead
//!
//! # Integration with Fan Control
//!
//! Effects integrate seamlessly with fan control:
//! - **Temperature Responsive**: Colors change based on actual sensor readings
//! - **Performance Indication**: Visual feedback for fan speeds and curves
//! - **Status Display**: Different colors for different operational states
//! - **Error Indication**: Special patterns for hardware or configuration errors

pub mod effect_runner;
