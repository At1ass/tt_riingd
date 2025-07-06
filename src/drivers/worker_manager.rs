use anyhow::{Ok, Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::drivers::fan_controller::FanController;

use super::{ColorBufferForSend, ControllerColorBuffer, ControllerColorBufferForSend};

#[derive(Debug)]
enum ControllerCommand {
    SetSpeeds {
        data: Vec<(usize, u8)>,
        result_tx: oneshot::Sender<Result<()>>,
    },
    SetColors {
        data: ControllerColorBuffer,
        result_tx: oneshot::Sender<Result<()>>,
    },
    Init {
        result_tx: oneshot::Sender<Result<()>>,
    },
    GetFirmware {
        result_tx: oneshot::Sender<Result<(u8, u8, u8)>>,
    },
    LedCount {
        result_tx: oneshot::Sender<Result<usize>>,
    },
}

struct ControllerWorker {
    controller_id: u8,
    controller: Arc<dyn FanController>,
    command_rx: mpsc::Receiver<ControllerCommand>,
    shutdown_rx: watch::Receiver<bool>,
}

impl ControllerWorker {
    async fn run(mut self) {
        info!("Controller worker {} starting", self.controller_id);

        loop {
            tokio::select! {
                // Приоритет shutdown сигналу
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("Controller worker {} received shutdown signal", self.controller_id);
                        break;
                    }
                }

                // Обработка команд
                command = self.command_rx.recv() => {
                    match command {
                        Some(ControllerCommand::SetSpeeds { data, result_tx }) => {
                            let result = self.handle_set_speeds(data).await;
                            let _ = result_tx.send(result);
                        }
                        Some(ControllerCommand::SetColors { data, result_tx }) => {
                            let result = self.handle_set_colors(data).await;
                            let _ = result_tx.send(result);
                        }
                        Some(ControllerCommand::Init { result_tx }) => {
                            let result = self.controller.send_init().await;
                            let _ = result_tx.send(result);
                        }
                        Some(ControllerCommand::GetFirmware { result_tx }) => {
                            let result = self.handle_get_firmware().await;
                            let _ = result_tx.send(result);
                        }
                        Some(ControllerCommand::LedCount { result_tx }) => {
                            let result = self.controller.led_count();
                            let _ = result_tx.send(Ok(result));
                        }
                        None => {
                            info!("Controller worker {} command channel closed", self.controller_id);
                            break;
                        }
                    }
                }
            }
        }

        info!("Controller worker {} stopped", self.controller_id);
    }

    async fn handle_set_speeds(&self, data: Vec<(usize, u8)>) -> Result<()> {
        self.controller.update_speed_batch(&data).await
    }

    async fn handle_set_colors(&self, data: ControllerColorBuffer) -> Result<()> {
        let batch_refs: ControllerColorBufferForSend = data
            .iter()
            .map(|(ch, colors)| (*ch, colors.as_slice()))
            .collect();

        self.controller.update_color_batch(&batch_refs).await
    }

    // async fn handle_init(&self) -> Result<()> {
    //     self.controller.send_init().await
    // }

    async fn handle_get_firmware(&self) -> Result<(u8, u8, u8)> {
        self.controller.firmware_version().await
    }
}

#[derive(Debug)]
pub struct ControllerWorkerManager {
    workers: HashMap<u8, mpsc::Sender<ControllerCommand>>,
    worker_handles: Vec<JoinHandle<()>>,
    shutdown_tx: watch::Sender<bool>,
}

impl ControllerWorkerManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = watch::channel(false);

        Self {
            workers: HashMap::new(),
            worker_handles: Vec::new(),
            shutdown_tx,
        }
    }

    pub async fn initialize(&mut self, controllers: Vec<Arc<dyn FanController>>) -> Result<()> {
        for (index, controller) in controllers.into_iter().enumerate() {
            let controller_id = (index + 1) as u8;

            let (command_tx, command_rx) = mpsc::channel(32);

            let worker = ControllerWorker {
                controller_id,
                controller,
                command_rx,
                shutdown_rx: self.shutdown_tx.subscribe(),
            };

            let handle = tokio::spawn(async move {
                worker.run().await;
            });

            self.workers.insert(controller_id, command_tx);
            self.worker_handles.push(handle);
        }

        info!("Initialized {} controller workers", self.workers.len());
        Ok(())
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub async fn send_init(&self, controller_id: u8) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&controller_id)
            .ok_or_else(|| anyhow!("Controller {controller_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::Init { result_tx };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub async fn set_colors(
        &self,
        controller_id: u8,
        colors: &[ColorBufferForSend<'_>],
    ) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&controller_id)
            .ok_or_else(|| anyhow!("Controller {controller_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetColors {
            data: colors
                .iter()
                .map(|(ch, rgb_slice)| (*ch, rgb_slice.to_vec()))
                .collect(),
            result_tx,
        };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub async fn set_speeds(&self, controller_id: u8, speeds: &[(usize, u8)]) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&controller_id)
            .ok_or_else(|| anyhow!("Controller {controller_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetSpeeds {
            data: speeds.to_vec(),
            result_tx,
        };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub fn try_set_colors(&self, controller_id: u8, colors: &[ColorBufferForSend<'_>]) -> bool {
        if let Some(worker_tx) = self.workers.get(&controller_id) {
            let (result_tx, _) = oneshot::channel();
            let command = ControllerCommand::SetColors {
                data: colors
                    .iter()
                    .map(|(ch, rgb_slice)| (*ch, rgb_slice.to_vec()))
                    .collect(),
                result_tx,
            };

            worker_tx.try_send(command).is_ok()
        } else {
            false
        }
    }

    pub fn try_set_speeds(&self, controller_id: u8, speeds: &[(usize, u8)]) -> bool {
        if let Some(worker_tx) = self.workers.get(&controller_id) {
            let (result_tx, _) = oneshot::channel();
            let command = ControllerCommand::SetSpeeds {
                data: speeds.to_vec(),
                result_tx,
            };

            worker_tx.try_send(command).is_ok()
        } else {
            false
        }
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down controller workers");
        let _ = self.shutdown_tx.send(true);

        for handle in self.worker_handles.drain(..) {
            if let Err(e) = handle.await {
                error!("Worker join error: {}", e);
            }
        }

        self.workers.clear();
    }

    pub async fn get_firmware(&self, controller: u8) -> Result<(u8, u8, u8)> {
        let worker_tx = self
            .workers
            .get(&controller)
            .ok_or_else(|| anyhow!("Controller {controller} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::GetFirmware { result_tx };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub async fn led_count(&self, controller: u8) -> Result<usize> {
        let worker_tx = self
            .workers
            .get(&controller)
            .ok_or_else(|| anyhow!("Controller {controller} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::LedCount { result_tx };

        worker_tx.send(command).await?;

        result_rx.await?
    }
}
