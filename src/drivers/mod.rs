use std::sync::LazyLock;

use hidapi::HidApi;
use tracing::{info, warn};

pub mod commands;
pub mod controller_manager;
pub mod fan_controller;
pub mod registry;
pub mod tt_riing_quad;
mod worker_manager;

type ColorBuffer = (usize, Vec<(u8, u8, u8)>);
type ControllerColorBuffer = Vec<ColorBuffer>;
type ColorBufferForSend<'a> = (usize, &'a [(u8, u8, u8)]);
type ControllerColorBufferForSend<'a> = Vec<ColorBufferForSend<'a>>;

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
