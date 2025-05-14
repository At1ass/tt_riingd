use std::{slice::Iter as SliceIter, sync::Arc};

use anyhow::{Ok, Result, anyhow};
use futures::stream::{Iter as FutureIter, StreamExt, iter};
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
        self.async_iter()
            .fold(Ok(()), |acc, device| async {
                acc.and(device.send_init().await)
            })
            .await
    }

    pub async fn update_speeds(&self, temp: f32) -> Result<()> {
        self.async_iter()
            .fold(Ok(()), |acc, device| async {
                acc.and(device.update_speeds(temp).await)
            })
            .await
    }

    pub async fn switch_curve(&self, controller: u8, channel: u8, curve: &str) -> Result<()> {
        self.get_device(controller)?
            .switch_curve(channel, curve)
            .await
    }

    pub async fn get_active_curve(&self, controller: u8, channel: u8) -> Result<String> {
        self.get_device(controller)?.get_active_curve(channel).await
    }

    pub async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_curve_data(channel, curve, curve_data).await
    }

    fn get_device(&self, controller: u8) -> Result<&Box<dyn FanController>> {
        self.0
            .iter()
            .enumerate()
            .find(|(idx, _)| idx + 1 == controller as usize)
            .map(|(_, device)| device)
            .ok_or(anyhow! {"Device {controller} not found"})
    }

    fn async_iter(&self) -> FutureIter<SliceIter<'_, Box<dyn FanController>>> {
        iter(self.0.iter())
    }
}
