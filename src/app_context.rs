use std::{path::PathBuf, sync::Arc};

#[cfg(debug_assertions)]
use log::info;
use once_cell::sync::Lazy;
use anyhow::Result;

use crate::{config::{self, ColorCfg}, controller, mappings::{ColorMapping, Mapping}, sensors::TemperatureSensor, temperature_sensors::lm_sensor};

pub struct AppContext {
    pub cfg: config::Config,
    pub controllers: controller::Controllers,
    pub sensors: Vec<Box<dyn TemperatureSensor>>,
    pub mapping: Arc<Mapping>,
    pub colors: Arc<Vec<ColorCfg>>,
    pub color_mappings: Arc<ColorMapping>,
}

pub struct LMSensorsRef(pub lm_sensors::LMSensors);

unsafe impl Sync for LMSensorsRef {}
unsafe impl Send for LMSensorsRef {}

pub static LMSENSORS: Lazy<LMSensorsRef> = Lazy::new(|| {
    LMSensorsRef(
        lm_sensors::Initializer::default()
            .initialize()
            .expect("Cannot initialize lm-sensors"),
    )
});

pub async fn init_context(config_path: Option<PathBuf>) -> Result<AppContext> {
    let config = config::load(config_path)?;
    let controllers = controller::Controllers::init_from_cfg(&config)?;
    let sensors = lm_sensor::LmSensorSource::discover(&LMSENSORS.0, &config.sensors)?;

    #[cfg(debug_assertions)]
    {
        info!("Loaded {} temperature sensors", sensors.len());
    }

    let mapping = Arc::new(Mapping::load_mappings(&config.mappings));
    let colors = Arc::new(config.colors.clone());
    let color_mappings = Arc::new(ColorMapping::build_color_mapping(&config.color_mappings));

    Ok(AppContext {
        cfg: config,
        controllers,
        sensors,
        mapping,
        colors,
        color_mappings,
    })
}

