use crate::fan_curve::FanCurve;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait FanController: Send + Sync + core::fmt::Debug {
    async fn send_init(&self) -> Result<()>;

    async fn update_speeds(&self, temp: f32) -> Result<()>;
    async fn update_channel(&self, _channel: u8, temp: f32) -> Result<()> {
        self.update_speeds(temp).await
    }
    async fn update_channel_color(&self, _channel: u8, red: u8, green: u8, blue: u8) -> Result<()>;
    async fn switch_curve(&self, channel: u8, curve: &str) -> Result<()>;
    async fn get_active_curve(&self, channel: u8) -> Result<String>;
    async fn update_curve_data(
        &self,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()>;
}
