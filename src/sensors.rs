use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait TemperatureSensor: Send + Sync {
    async fn read_temperature(&self) -> Result<f32>;
    async fn sensor_name(&self) -> Option<String> {
        None
    }
}
