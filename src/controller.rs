use std::future;
use std::sync::Arc;

use anyhow::{Context, Ok, Result};
use futures::stream::{StreamExt, TryStreamExt, iter};
use hidapi::HidApi;

use crate::fan_curve::FanCurve;
use crate::{drivers, fan_controller::FanController};

#[derive(Debug, Clone)]
pub struct Controllers(Arc<Vec<Box<dyn FanController>>>);

impl Controllers {
    pub fn init(init_speed: u8) -> Result<Self> {
        let api = HidApi::new()?;
        let mut controllers = Vec::<Box<dyn FanController>>::new();

        controllers.extend(drivers::tt_riing_quad::TTRiingQuad::probe(
            &api, init_speed,
        )?);

        Ok(Self(Arc::new(controllers)))
    }

    pub async fn send_init(&self) -> Result<()> {
        let clonned = self.0.clone();
        iter(clonned.iter())
            .map(Ok)
            .try_for_each(|device| async { device.send_init().await })
            .await
    }

    pub async fn update_speeds(&self, temp: f32) -> Result<()> {
        let clonned = self.0.clone();
        iter(clonned.iter())
            .map(Ok)
            .try_for_each(|device| async { device.update_speeds(temp).await })
            .await
    }

    pub async fn switch_curve(&self, controller: u8, channel: u8, curve: &str) -> Result<()> {
        let clonned = self.0.clone();
        iter(clonned.iter().enumerate())
            .filter(|(idx, _)| future::ready(idx + 1 == controller as usize))
            .map(|(_, device)| device)
            .map(Ok)
            .try_for_each(|device| async { device.switch_curve(channel, curve).await })
            .await
    }

    pub async fn get_active_curve(&self, controller: u8, channel: u8) -> Result<String> {
        let clonned = self.0.clone();
        let dev_opt = iter(clonned.iter().enumerate())
            .filter(|(idx, _)| future::ready(idx + 1 == controller as usize))
            .map(|(_, device)| device)
            .map(Ok)
            .try_next()
            .await?;

        let dev = dev_opt.context("Controller not found")?;

        dev.get_active_curve(channel).await

    }

    pub async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()> {
        let clonned = self.0.clone();
        iter(clonned.iter().enumerate())
            .filter(|(idx, _)| future::ready(idx + 1 == controller as usize))
            .map(|(_, device)| device)
            .map(Ok)
            .try_for_each(|device| async { device.update_curve_data(channel, curve, curve_data).await })
            .await
    }
}
