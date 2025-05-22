use dashmap::{DashMap, DashSet};

use crate::config::{ColorMappingCfg, MappingCfg};

pub type SensorKey = String;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FanRef {
    pub controller_id: usize,
    pub channel: usize,
}

#[derive(Default, Debug)]
pub struct Mapping {
    fans2sensor: DashMap<FanRef, SensorKey>,
    sensor2fans: DashMap<SensorKey, DashSet<FanRef>>,
}

#[derive(Default, Debug)]
pub struct ColorMapping {
    color2fans: DashMap<String, DashSet<FanRef>>,
}

impl ColorMapping {
    pub fn build_color_mapping(color_cfg: &[ColorMappingCfg]) -> Self {
        color_cfg
            .iter()
            .flat_map(|c| {
                let ckey = c.color.clone();
                c.targets.iter().map(move |t| (ckey.clone(), t))
            })
            .fold(Self::default(), |acc, (sensor, target)| {
                let fan = FanRef {
                    controller_id: target.controller as usize,
                    channel: target.fan_idx as usize,
                };

                acc.color2fans.entry(sensor).or_default().insert(fan);
                acc
            })
    }

    pub fn iter(&self) -> dashmap::iter::Iter<String, DashSet<FanRef>> {
        self.color2fans.iter()
    }
}

impl Mapping {
    pub fn load_mappings(mapping_cfg: &[MappingCfg]) -> Self {
        mapping_cfg
            .iter()
            .flat_map(|m| {
                let skey = m.sensor.clone();
                m.targets.iter().map(move |t| (skey.clone(), t))
            })
            .fold(Self::default(), |acc, (sensor, target)| {
                let fan = FanRef {
                    controller_id: target.controller as usize,
                    channel: target.fan_idx as usize,
                };

                acc.fans2sensor.insert(fan, sensor.clone());
                acc.sensor2fans.entry(sensor).or_default().insert(fan);
                acc
            })
    }

    pub fn attach(&self, fan: FanRef, sensor: SensorKey) {
        if let Some(old) = self.fans2sensor.insert(fan, sensor.clone()) {
            if let Some(set) = self.sensor2fans.get(&old) {
                set.remove(&fan);
            }
        }
        self.sensor2fans.entry(sensor).or_default().insert(fan);
    }

    pub fn detach(&self, fan: FanRef) {
        if let Some((_, key)) = self.fans2sensor.remove(&fan) {
            if let Some(set) = self.sensor2fans.get(&key) {
                set.remove(&fan);
            }
        }
    }

    pub fn fans_for_sensor<'a>(
        &'a self,
        sensor: &'a SensorKey,
    ) -> impl Iterator<Item = FanRef> + 'a {
        self.sensor2fans
            .get(sensor)
            .into_iter()
            .flat_map(|set| set.iter().map(|r| *r).collect::<Vec<_>>())
    }
}
