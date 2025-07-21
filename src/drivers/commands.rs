use crate::buffer::{ColorSnapshot, SpeedSnapshot};

pub enum BatchCommand {
    SetColors { data: ColorSnapshot },
    SetSpeeds { data: SpeedSnapshot },
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
