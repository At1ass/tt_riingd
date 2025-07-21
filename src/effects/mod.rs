//! RGB lighting effects and visual control system.
//!
//! Stream-based RGB lighting effect system for fan controllers with LED support.
//! Effects are implemented as async streams that generate RGB color values over time.
//!
//! Provides static effects (constant colors) and dynamic effects (rainbow, breathing, fade).
//! Effects are configured through YAML and support temperature-responsive coloring.
//!
//! For detailed documentation, examples, and configuration, see the
//! [Effects System Guide](https://docs.tt-riingd.rs/effects/)

pub mod effect_runner;
