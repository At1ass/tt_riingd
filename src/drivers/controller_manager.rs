//! Hardware controller management for Thermaltake Riing fans.
//!
//! Provides high-level interface for controlling fan speed and RGB lighting
//! through HID communication with Thermaltake devices.

use std::{collections::HashMap, sync::Arc};

use anyhow::{Ok, Result};
use tracing::{error, warn};

use crate::{config::Config, drivers, drivers::fan_controller::FanController};

use super::{
    ControllerColorBufferForSend, HIDAPI,
    commands::{BatchCommand, BatchResult, ControllerBatchStats, ExecutionMode},
    worker_manager::ControllerWorkerManager,
};

/// Thread-safe collection of fan controllers.
///
/// Manages multiple hardware fan controllers and provides a unified interface
/// for controlling fan speeds, RGB lighting, and curve management across all
/// connected devices.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::drivers::controller_manager::ControllerManager;
/// use tt_riingd::config::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let controllers = ControllerManager::init_from_cfg(&config)?;
///
/// // Initialize all controllers
/// controllers.send_init().await?;
///
/// // Update fan speed based on temperature
/// controllers.update_channel(1, 1, 45.0, 50).await?;
/// # Ok(())
/// # }
/// ```
type ControllerColorBuffer = Vec<(usize, Vec<(u8, u8, u8)>)>;

#[derive(Debug)]
pub struct ControllerManager {
    worker_manager: ControllerWorkerManager,
}

impl ControllerManager {
    /// Creates empty Controllers for testing purposes.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            worker_manager: ControllerWorkerManager::new(),
        }
    }

    /// Creates Controllers from configuration file.
    ///
    /// Initializes controllers based on the provided configuration, including
    /// device selection, fan curves, and initial settings.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Configuration containing controller and curve definitions
    ///
    /// # Errors
    ///
    /// Returns an error if device initialization fails or configuration is invalid.
    pub async fn init_from_cfg(cfg: &Config) -> Result<Self> {
        let mut controllers = Vec::<Arc<dyn FanController>>::new();

        match HIDAPI.as_ref() {
            Some(hidapi) => {
                controllers.extend(drivers::tt_riing_quad::TTRiingQuad::find_controllers(
                    hidapi,
                    &cfg.controllers,
                )?);
            }
            None => {
                warn!("HID API not available, no hardware controllers will be initialized");
            }
        }

        let mut worker_manager = ControllerWorkerManager::new();
        worker_manager.initialize(controllers).await?;

        Ok(Self { worker_manager })
    }

    pub async fn shutdown(&mut self) {
        self.worker_manager.shutdown().await
    }

    pub async fn batch_update(
        &self,
        command: BatchCommand<'_>,
        mode: ExecutionMode,
    ) -> Result<BatchResult> {
        match command {
            BatchCommand::SetColors { data } => {
                let stats = self.handle_set_colors(data, mode).await?;
                Ok(BatchResult::ColorsSet(stats))
            }
            BatchCommand::SetSpeeds { data } => {
                let stats = self.handle_set_speeds(data, mode).await?;
                Ok(BatchResult::SpeedsSet(stats))
            }
            BatchCommand::Init => {
                let stats = self.handle_init().await?;
                Ok(BatchResult::ControllersInitialized(stats))
            }
            BatchCommand::GetFirmwares => {
                let (stats, firmware_data) = self.handle_firmware_versions().await?;
                Ok(BatchResult::FirmwareRetrieved {
                    stats,
                    firmware_data,
                })
            }
        }
    }

    pub async fn led_count(&self, controller: u8) -> Result<usize> {
        self.worker_manager.led_count(controller).await
    }

    async fn handle_init(&self) -> Result<ControllerBatchStats> {
        let total = self.worker_manager.worker_count();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = (1..=total)
            .map(|controller_id| async move {
                let result = self.worker_manager.send_init(controller_id as u8).await;
                (controller_id as u8, result)
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} initialization failed: {}", controller_id, e);
                failed_controllers.push(controller_id);
            } else {
                successful += 1;
            }
        }

        Ok(ControllerBatchStats {
            total,
            successful,
            failed: total - successful,
            failed_controllers,
        })
    }

    async fn handle_firmware_versions(
        &self,
    ) -> Result<(ControllerBatchStats, Vec<(u8, (u8, u8, u8))>)> {
        let total = self.worker_manager.worker_count();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = (0..total)
            .map(|controller_id| async move {
                let result = self.worker_manager.get_firmware(controller_id as u8).await;
                (controller_id as u8, result)
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results.iter() {
            if let Err(e) = result {
                error!("Controller {} initialization failed: {}", controller_id, e);
                failed_controllers.push(*controller_id);
            } else {
                successful += 1;
            }
        }

        let results = results
            .into_iter()
            .map(|(id, res)| {
                res.map(|version| (id, version))
                    .unwrap_or_else(|_| (id, (0, 0, 0))) // Default version if error
            })
            .collect();

        Ok((
            ControllerBatchStats {
                total,
                successful,
                failed: total - successful,
                failed_controllers,
            },
            results,
        ))
    }

    async fn handle_set_colors(
        &self,
        color_data: &HashMap<u8, ControllerColorBuffer>,
        mode: ExecutionMode,
    ) -> Result<ControllerBatchStats> {
        match mode {
            ExecutionMode::Blocking => self.set_colors_blocking(color_data).await,
            ExecutionMode::FireAndForget => Ok(self.set_colors_fire_and_forget(color_data)),
        }
    }

    async fn handle_set_speeds(
        &self,
        speed_data: &HashMap<u8, Vec<(usize, u8)>>,
        mode: ExecutionMode,
    ) -> Result<ControllerBatchStats> {
        match mode {
            ExecutionMode::Blocking => self.set_speeds_blocking(speed_data).await,
            ExecutionMode::FireAndForget => Ok(self.set_speeds_fire_and_forget(speed_data)),
        }
    }

    async fn set_colors_blocking(
        &self,
        color_data: &HashMap<u8, ControllerColorBuffer>,
    ) -> Result<ControllerBatchStats> {
        let total = color_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = color_data
            .iter()
            .map(|(controller_id, buffer)| {
                let batch_refs: ControllerColorBufferForSend = buffer
                    .iter()
                    .map(|(channel, colors)| (*channel, colors.as_slice()))
                    .collect();

                async move {
                    let result = self
                        .worker_manager
                        .set_colors(*controller_id, &batch_refs)
                        .await;
                    (*controller_id, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} color update failed: {}", controller_id, e);
                failed_controllers.push(controller_id);
            } else {
                successful += 1;
            }
        }

        Ok(ControllerBatchStats {
            total,
            successful,
            failed: total - successful,
            failed_controllers,
        })
    }

    async fn set_speeds_blocking(
        &self,
        speed_data: &HashMap<u8, Vec<(usize, u8)>>,
    ) -> Result<ControllerBatchStats> {
        let total = speed_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = speed_data
            .iter()
            .map(|(controller_id, speeds)| async move {
                let result = self
                    .worker_manager
                    .set_speeds(*controller_id, speeds.as_slice())
                    .await;
                (*controller_id, result)
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} speed update failed: {}", controller_id, e);
                failed_controllers.push(controller_id);
            } else {
                successful += 1;
            }
        }

        Ok(ControllerBatchStats {
            total,
            successful,
            failed: total - successful,
            failed_controllers,
        })
    }

    fn set_colors_fire_and_forget(
        &self,
        color_data: &HashMap<u8, ControllerColorBuffer>,
    ) -> ControllerBatchStats {
        let total = color_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        for (controller_id, buffer) in color_data {
            let batch_refs: ControllerColorBufferForSend = buffer
                .iter()
                .map(|(channel, colors)| (*channel, colors.as_slice()))
                .collect();

            if self
                .worker_manager
                .try_set_colors(*controller_id, &batch_refs)
            {
                successful += 1;
            } else {
                failed_controllers.push(*controller_id);
            }
        }

        ControllerBatchStats {
            total,
            successful,
            failed: total - successful,
            failed_controllers,
        }
    }

    fn set_speeds_fire_and_forget(
        &self,
        speed_data: &HashMap<u8, Vec<(usize, u8)>>,
    ) -> ControllerBatchStats {
        let total = speed_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        for (controller_id, speeds) in speed_data {
            if self
                .worker_manager
                .try_set_speeds(*controller_id, speeds.as_slice())
            {
                successful += 1;
            } else {
                failed_controllers.push(*controller_id);
            }
        }

        ControllerBatchStats {
            total,
            successful,
            failed: total - successful,
            failed_controllers,
        }
    }
}
