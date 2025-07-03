use std::sync::LazyLock;

use hidapi::HidApi;
use tracing::{info, warn};

pub mod controller_manager;
pub mod fan_controller;
pub mod registry;
pub mod tt_riing_quad;

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
