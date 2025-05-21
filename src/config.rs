use crate::fan_curve::Point;
use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u8,
    #[serde(default = "defaults::tick_seconds")]
    pub tick_seconds: u16,
    #[serde(default = "defaults::enable_broadcast")]
    pub enable_broadcast: bool,
    #[serde(default = "defaults::broadcast_interval")]
    pub broadcast_interval: u16,
    #[serde(default)]
    pub controllers: Vec<ControllerCfg>,
    #[serde(default)]
    pub curves: Vec<CurveCfg>,
    #[serde(default)]
    pub sensors: Vec<SensorCfg>,
    #[serde(default)]
    pub mappings: Vec<MappingCfg>,
    #[serde(default)]
    pub colors: Vec<ColorCfg>,
    #[serde(default)]
    pub color_mappings: Vec<ColorMappingCfg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControllerCfg {
    RiingQuad {
        id: String,
        usb: UsbSelector,
        #[serde(default)]
        fans: Vec<FanCfg>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCfg {
    pub idx: u8,
    pub name: String,
    pub active_curve: String,
    // pub curve: HashMap<String, CurveCfg>,
    pub curve: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CurveCfg {
    Constant {
        id: String,
        speed: u8,
    },
    StepCurve {
        id: String,
        tmps: Vec<f32>,
        spds: Vec<u8>,
    },
    Bezier {
        id: String,
        points: Vec<Point>,
    },
}

impl CurveCfg {
    pub fn get_id(&self) -> String {
        match self {
            CurveCfg::Constant { id, .. } => id.clone(),
            CurveCfg::StepCurve { id, .. } => id.clone(),
            CurveCfg::Bezier { id, .. } => id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingCfg {
    pub sensor: String,
    pub targets: Vec<FanTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMappingCfg {
    pub color: String,
    pub targets: Vec<FanTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanTarget {
    pub controller: u8,
    pub fan_idx: u8,
}

mod defaults {
    pub fn tick_seconds() -> u16 {
        2
    }
    pub fn enable_broadcast() -> bool {
        false
    }
    pub fn broadcast_interval() -> u16 {
        2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbSelector {
    pub vid: u16,
    pub pid: u16,
    #[serde(default)]
    pub serial: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SensorCfg {
    LmSensors {
        id: String,
        chip: String,
        feature: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorCfg {
    pub color: String,
    pub rgb: [u8;3],
}

fn locate_config() -> Result<PathBuf> {
    // 2) ENV
    if let Ok(env_path) = env::var("TT_RIINGD_CONFIG") {
        return Ok(PathBuf::from(env_path))
    }

    // 3) XDG_CONFIG_HOME или $HOME/.config
    if let Some(mut cfg_dir) = env::var_os("XDG_CONFIG_HOME")
                                 .map(PathBuf::from)
                                 .or_else(|| env::var_os("HOME")
                                         .map(|h| Path::new(&h).join(".config"))) {
        cfg_dir.push("tt_riingd/config.yml");
        if cfg_dir.exists() {
            return Ok(cfg_dir.clone())
        }
    }

    // 4) /etc
    let etc = Path::new("/etc/tt_riingd/config.yml");
    if etc.exists() {
        return Ok(etc.to_path_buf())
    }

    anyhow::bail!("файл конфигурации не найден ни в одном из стандартных мест")
}

pub fn load(path: Option<PathBuf>) -> Result<Config> {
    let path = path.unwrap_or_else(|| locate_config().expect("Failed to load config"));
    info!("Used config: {}", path.display());
    let txt = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: Config = serde_yaml::from_str(&txt).context("parse YAML")?;
    if cfg.version != 1 {
        anyhow::bail!("unsupported config version {}", cfg.version);
    }
    Ok(cfg)
}

#[allow(dead_code)]
pub fn save(path: &Path, cfg: &Config) -> Result<()> {
    let tmp = path.with_extension("yml.tmp");
    fs::write(&tmp, serde_yaml::to_string(cfg)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}
