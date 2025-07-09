//! Hardware controller management for Thermaltake Riing fans.
//!
//! Provides high-level interface for controlling fan speed and RGB lighting
//! through HID communication with Thermaltake devices.

use std::collections::HashMap;

use anyhow::{Ok, Result, anyhow};
use dashmap::DashMap;
use futures::StreamExt;
use tracing::{debug, error, warn};

use crate::config::Config;

use super::{
    ControllerColorBufferForSend, HardwareFingerprint,
    commands::{BatchCommand, BatchResult, ControllerBatchStats, ExecutionMode},
    registry::Registry,
    worker_manager::ControllerWorkerManager,
};

/// Thread-safe collection of fan controllers.
///
/// Manages multiple hardware fan controllers and provides a unified interface
/// for controlling fan speeds, RGB lighting, and curve management across all
/// connected devices.
///
/// See integration tests for usage examples.
type ControllerColorBuffer = Vec<(usize, Vec<(u8, u8, u8)>)>;

macro_rules! get_mapping_with_check {
    ($self:ident, $key:expr, |$val:ident| $then:expr) => {
        if let Some($val) = $self.controller_id2worker_id.get($key) {
            $then
        } else {
            Err(anyhow::anyhow!("Controller {} not found", $key))
        }
    };
}

#[derive(Debug)]
pub struct ControllerManager {
    worker_manager: ControllerWorkerManager,
    controller_id2worker_id: DashMap<String, Option<u32>>,
    fingerprint2controller_id: DashMap<HardwareFingerprint, String>,
}

impl ControllerManager {
    /// Creates empty Controllers for testing purposes.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            worker_manager: ControllerWorkerManager::new(),
            controller_id2worker_id: DashMap::new(),
            fingerprint2controller_id: DashMap::new(),
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
        let controllers = Registry::build_controllers_from_config(cfg).await?;

        let fingerprint2controller_id = futures::stream::iter(&controllers)
            .then(|controller| async move {
                let device_info = controller.get_device_info().await.unwrap();
                let fingerprint = HardwareFingerprint {
                    vendor_id: device_info.vendor_id(),
                    product_id: device_info.product_id(),
                    serial: device_info.serial_number().map(|s| s.to_string()),
                };
                let controller_id = controller.get_id().await;
                (fingerprint, controller_id)
            })
            .collect::<DashMap<_, _>>()
            .await;

        let controller_id2worker_id: DashMap<String, Option<u32>> = DashMap::new();
        for (index, controller) in controllers.iter().enumerate() {
            let controller_id = controller.get_id().await;
            controller_id2worker_id.insert(controller_id.clone(), Some((index + 1) as u32));
        }

        let mut worker_manager = ControllerWorkerManager::new();
        worker_manager.initialize(controllers).await?;

        Ok(Self {
            worker_manager,
            controller_id2worker_id,
            fingerprint2controller_id,
        })
    }

    pub async fn shutdown(&mut self) {
        self.worker_manager.shutdown().await
    }

    pub async fn stop_controller_worker(&mut self, controller_id: String) -> Result<()> {
        get_mapping_with_check!(self, &controller_id, |worker_id| {
            if let Some(wid) = *worker_id {
                debug!("Stopping worker for controller {}", controller_id);
                self.worker_manager.stop_worker(wid).await
            } else {
                debug!("Worker for controller {} already stopped", controller_id);
                Ok(())
            }
        })
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

    pub async fn device_disconnected(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        serial: Option<String>,
    ) -> Result<()> {
        if let Some(controller_id) = self.fingerprint2controller_id.get(&HardwareFingerprint {
            vendor_id,
            product_id,
            serial,
        }) {
            let controller_id = controller_id.value();
            debug!("Removing controller with ID: {}", controller_id);
            if let Some(mut worker_id) = self.controller_id2worker_id.get_mut(controller_id) {
                if let Some(wid) = *worker_id {
                    debug!("Stopping worker for controller {}", controller_id);
                    self.worker_manager.stop_worker(wid).await?;
                    *worker_id = None;
                } else {
                    debug!("Worker for controller {} already stopped", controller_id);
                }
            } else {
                warn!("Controller ID {} not found in mapping", controller_id);
            }
            Ok(())
        } else {
            Err(anyhow!(
                "No controller found for fingerprint: ({}, {})",
                vendor_id,
                product_id
            ))
        }
    }

    pub async fn device_connected(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        serial: Option<String>,
    ) -> Result<()> {
        let fingerprint = HardwareFingerprint {
            vendor_id,
            product_id,
            serial,
        };

        let connected_controller =
            Registry::build_controller_from_fingerprint(&fingerprint).await?;
        let controller_id = connected_controller.get_id().await;

        let worker_id = self
            .worker_manager
            .start_worker(connected_controller)
            .await?;

        self.controller_id2worker_id
            .insert(controller_id.clone(), Some(worker_id));
        self.fingerprint2controller_id
            .insert(fingerprint, controller_id.clone());

        debug!("Controller with ID {} connected", controller_id);
        Ok(())
    }

    pub async fn led_count(&self, controller_id: &String) -> Result<usize> {
        get_mapping_with_check!(self, controller_id, |worker_id| {
            if let Some(wid) = *worker_id {
                debug!("Getting LED count for controller {}", controller_id);
                self.worker_manager.led_count(wid).await
            } else {
                Err(anyhow!(
                    "Worker for controller {} is not initialized",
                    controller_id
                ))
            }
        })
    }

    async fn handle_init(&self) -> Result<ControllerBatchStats> {
        let total = self.worker_manager.worker_count();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = (1..=total)
            .map(|controller_id| async move {
                let result = self.worker_manager.send_init(controller_id as u32).await;
                (controller_id as u32, result)
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} initialization failed: {}", controller_id, e);
                if let Some(controller_id) = self
                    .controller_id2worker_id
                    .iter()
                    .find(|entry| entry.value() == &Some(controller_id))
                    .map(|entry| entry.key().clone())
                {
                    failed_controllers.push(controller_id);
                } else {
                    error!("Controller ID {} not found in mapping", controller_id);
                }
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
    ) -> Result<(ControllerBatchStats, Vec<(String, (u8, u8, u8))>)> {
        let total = self.worker_manager.worker_count();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = (1..=total)
            .map(|controller_id| async move {
                let result = self.worker_manager.get_firmware(controller_id as u32).await;
                if let Some(controller_id) = self
                    .controller_id2worker_id
                    .iter()
                    .find(|entry| entry.value() == &Some(controller_id as u32))
                    .map(|entry| entry.key().clone())
                {
                    (controller_id, result)
                } else {
                    error!("Controller ID {} not found in mapping", controller_id);
                    (String::new(), Err(anyhow::anyhow!("Controller not found")))
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (controller_id, result) in results.iter() {
            if let Err(e) = result {
                error!("Controller {} initialization failed: {}", controller_id, e);
                failed_controllers.push(controller_id.clone());
            } else {
                successful += 1;
            }
        }

        let results = results
            .into_iter()
            .map(|(id, res)| {
                res.map(|version| (id.clone(), version))
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
        color_data: &HashMap<String, ControllerColorBuffer>,
        mode: ExecutionMode,
    ) -> Result<ControllerBatchStats> {
        match mode {
            ExecutionMode::Blocking => self.set_colors_blocking(color_data).await,
            ExecutionMode::FireAndForget => Ok(self.set_colors_fire_and_forget(color_data)),
        }
    }

    async fn handle_set_speeds(
        &self,
        speed_data: &HashMap<String, Vec<(usize, u8)>>,
        mode: ExecutionMode,
    ) -> Result<ControllerBatchStats> {
        match mode {
            ExecutionMode::Blocking => self.set_speeds_blocking(speed_data).await,
            ExecutionMode::FireAndForget => Ok(self.set_speeds_fire_and_forget(speed_data)),
        }
    }

    async fn set_colors_blocking(
        &self,
        color_data: &HashMap<String, ControllerColorBuffer>,
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
                    if let Some(worker_id) = self.controller_id2worker_id.get(controller_id) {
                        debug!("Setting colors for controller {}", controller_id);
                        if let Some(worker_id) = *worker_id {
                            // Use the worker manager to set colors
                            debug!("Worker ID for controller {}: {}", controller_id, worker_id);
                            let result =
                                self.worker_manager.set_colors(worker_id, &batch_refs).await;
                            Some((controller_id, result))
                        } else {
                            error!("Worker ID for controller {} is None", controller_id);
                            None
                        }
                    } else {
                        error!("Controller ID {} not found in mapping", controller_id);
                        Some((controller_id, Err(anyhow::anyhow!("Controller not found"))))
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} color update failed: {}", controller_id, e);
                failed_controllers.push(controller_id.clone());
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
        speed_data: &HashMap<String, Vec<(usize, u8)>>,
    ) -> Result<ControllerBatchStats> {
        let total = speed_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        let futures: Vec<_> = speed_data
            .iter()
            .map(|(controller_id, speeds)| async move {
                if let Some(worker_id) = self.controller_id2worker_id.get(controller_id) {
                    debug!("Setting speeds for controller {}", controller_id);
                    if let Some(worker_id) = *worker_id {
                        // Use the worker manager to set colors
                        debug!("Worker ID for controller {}: {}", controller_id, worker_id);
                        let result = self
                            .worker_manager
                            .set_speeds(worker_id, speeds.as_slice())
                            .await;
                        Some((controller_id, result))
                    } else {
                        error!("Worker ID for controller {} is None", controller_id);
                        None
                    }
                } else {
                    error!("Controller ID {} not found in mapping", controller_id);
                    Some((controller_id, Err(anyhow::anyhow!("Controller not found"))))
                }
            })
            .collect();

        let results = futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        for (controller_id, result) in results {
            if let Err(e) = result {
                error!("Controller {} speed update failed: {}", controller_id, e);
                failed_controllers.push(controller_id.clone());
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
        color_data: &HashMap<String, ControllerColorBuffer>,
    ) -> ControllerBatchStats {
        let total = color_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        for (controller_id, buffer) in color_data {
            if let Some(worker_id) = self.controller_id2worker_id.get(controller_id) {
                debug!("Setting colors for controller {}", controller_id);

                let batch_refs: ControllerColorBufferForSend = buffer
                    .iter()
                    .map(|(channel, colors)| (*channel, colors.as_slice()))
                    .collect();

                if let Some(worker_id) = *worker_id {
                    if self.worker_manager.try_set_colors(worker_id, &batch_refs) {
                        successful += 1;
                    } else {
                        failed_controllers.push(controller_id.clone());
                    }
                } else {
                    error!("Worker ID for controller {} is None", controller_id);
                }
            } else {
                error!("Controller ID {} not found in mapping", controller_id);
                failed_controllers.push(controller_id.clone());
                continue;
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
        speed_data: &HashMap<String, Vec<(usize, u8)>>,
    ) -> ControllerBatchStats {
        let total = speed_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();

        for (controller_id, speeds) in speed_data {
            if let Some(worker_id) = self.controller_id2worker_id.get(controller_id) {
                debug!("Setting speeds for controller {}", controller_id);
                if let Some(worker_id) = *worker_id {
                    if self
                        .worker_manager
                        .try_set_speeds(worker_id, speeds.as_slice())
                    {
                        successful += 1;
                    } else {
                        failed_controllers.push(controller_id.clone());
                    }
                } else {
                    error!("Worker ID for controller {} is None", controller_id);
                }
            } else {
                error!("Controller ID {} not found in mapping", controller_id);
                failed_controllers.push(controller_id.clone());
                continue;
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
