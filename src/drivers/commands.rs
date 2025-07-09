use std::collections::HashMap;

use super::ControllerColorBuffer;

#[derive(Debug)]
pub enum BatchCommand<'a> {
    SetColors {
        data: &'a HashMap<String, ControllerColorBuffer>,
    },
    SetSpeeds {
        data: &'a HashMap<String, Vec<(usize, u8)>>,
    },
    Init,
    GetFirmwares,
}

#[derive(Debug)]
pub enum BatchResult {
    ColorsSet(ControllerBatchStats),
    SpeedsSet(ControllerBatchStats),
    ControllersInitialized(ControllerBatchStats),
    FirmwareRetrieved {
        stats: ControllerBatchStats,
        firmware_data: Vec<(String, (u8, u8, u8))>,
    },
}

#[derive(Debug)]
pub struct ControllerBatchStats {
    pub total: usize,
    pub successful: usize,
    pub failed: usize,
    pub failed_controllers: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ExecutionMode {
    Blocking,
    FireAndForget,
}
