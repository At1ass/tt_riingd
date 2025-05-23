use std::{collections::HashMap, slice::Iter as SliceIter, sync::Arc};

use anyhow::{Ok, Result, anyhow};
use futures::stream::{Iter as FutureIter, StreamExt, iter};
use hidapi::HidApi;
use once_cell::sync::Lazy;

use crate::{config::Config, drivers, fan_controller::FanController, fan_curve::FanCurve};

#[derive(Debug, Clone)]
pub struct Controllers(Arc<Vec<Box<dyn FanController>>>);

static HIDAPI: Lazy<HidApi> = Lazy::new(|| HidApi::new().expect("Failed to initialize hidapi"));

impl Controllers {
    #[allow(dead_code)]
    pub fn init(init_speed: u8) -> Result<Self> {
        let mut controllers = Vec::<Box<dyn FanController>>::new();

        controllers.extend(drivers::tt_riing_quad::TTRiingQuad::probe(
            &HIDAPI, init_speed,
        )?);

        Ok(Self(Arc::new(controllers)))
    }

    pub fn init_from_cfg(cfg: &Config) -> Result<Self> {
        let mut controllers = Vec::<Box<dyn FanController>>::new();
        let curve_map: HashMap<String, FanCurve> = cfg
            .curves
            .iter()
            .map(|c| (c.get_id(), FanCurve::from(c)))
            .collect();

        controllers.extend(drivers::tt_riing_quad::TTRiingQuad::find_controllers(
            &HIDAPI,
            &cfg.controllers,
            &curve_map,
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

    // pub async fn update_speeds(&self, temp: f32) -> Result<()> {
    //     self.async_iter()
    //         .fold(Ok(()), |acc, device| async {
    //             acc.and(device.update_speeds(temp).await)
    //         })
    //         .await
    // }

    pub async fn update_channel(&self, controller: u8, channel: u8, temp: f32) -> Result<()> {
        self.get_device(controller)?
            .update_channel(channel, temp)
            .await
    }

    pub async fn update_channel_color(
        &self,
        controller: u8,
        channel: u8,
        red: u8,
        green: u8,
        blue: u8,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_channel_color(channel, red, green, blue)
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

    pub async fn get_firmware_version(&self, controller: u8) -> Result<(u8, u8, u8)> {
        self.get_device(controller)?.firmware_version().await
    }

    pub async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_curve_data(channel, curve, curve_data)
            .await
    }

    #[allow(clippy::borrowed_box)]
    fn get_device(&self, controller: u8) -> Result<&Box<dyn FanController>> {
        self.0
            .iter()
            .enumerate()
            .find(|(idx, _)| idx + 1 == controller as usize)
            .map(|(_, device)| device)
            .ok_or(anyhow!("Device `{controller}` not found"))
    }

    fn async_iter(&self) -> FutureIter<SliceIter<'_, Box<dyn FanController>>> {
        iter(self.0.iter())
    }
}
