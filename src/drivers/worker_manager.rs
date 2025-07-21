use anyhow::{Ok, Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::buffer::ControllerEntry;
use crate::buffer::{ColorBufferData, SpeedBufferData};
use crate::drivers::fan_controller::FanController;

enum ControllerCommand {
    SetSpeeds {
        data: Arc<SpeedBufferData>,
        controller_entry: ControllerEntry,
        result_tx: oneshot::Sender<Result<()>>,
    },
    SetColors {
        data: Arc<ColorBufferData>,
        controller_entry: ControllerEntry,
        result_tx: oneshot::Sender<Result<()>>,
    },
    Init {
        result_tx: oneshot::Sender<Result<()>>,
    },
    GetFirmware {
        result_tx: oneshot::Sender<Result<(u8, u8, u8)>>,
    },
    Stop {
        result_tx: oneshot::Sender<Result<()>>,
    },
}

struct ControllerWorker {
    controller_id: u32,
    controller: Arc<dyn FanController>,
    command_rx: mpsc::Receiver<ControllerCommand>,
    shutdown_rx: watch::Receiver<bool>,
}

impl ControllerWorker {
    async fn run(mut self) {
        info!("Controller worker {} starting", self.controller_id);

        loop {
            tokio::select! {
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("Controller worker {} received shutdown signal", self.controller_id);
                        break;
                    }
                }

                command = self.command_rx.recv() => {
                    match command {
                        Some(ControllerCommand::SetSpeeds { data, controller_entry, result_tx }) => {
                            let result = self.handle_set_speeds(data, controller_entry).await;
                            let _ = result_tx.send(result);
                        }
                        Some(ControllerCommand::SetColors { data, controller_entry, result_tx }) => {
                            let result = self.handle_set_colors(data, controller_entry).await;
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
                        Some(ControllerCommand::Stop { result_tx }) => {
                            info!("Stopping controller worker {}", self.controller_id);
                            let _ = result_tx.send(Ok(()));
                            break;
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

    async fn handle_set_speeds(
        &self,
        data: Arc<SpeedBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        self.controller
            .update_speed_batch(data, controller_entry)
            .await
    }

    async fn handle_set_colors(
        &self,
        data: Arc<ColorBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        self.controller
            .update_color_batch(data, controller_entry)
            .await
    }

    async fn handle_get_firmware(&self) -> Result<(u8, u8, u8)> {
        self.controller.firmware_version().await
    }
}

#[derive(Debug)]
pub struct ControllerWorkerManager {
    workers: HashMap<u32, mpsc::Sender<ControllerCommand>>,
    worker_handles: Vec<JoinHandle<()>>,
    shutdown_tx: watch::Sender<bool>,
    next_worker_id: u32,
}

impl ControllerWorkerManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = watch::channel(false);

        Self {
            workers: HashMap::new(),
            worker_handles: Vec::new(),
            shutdown_tx,
            next_worker_id: 1,
        }
    }

    pub async fn initialize(&mut self, controllers: Vec<Arc<dyn FanController>>) -> Result<()> {
        for controller in controllers.into_iter() {
            let controller_id = self.next_worker_id;
            self.next_worker_id += 1;

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

    pub async fn send_init(&self, worker_id: u32) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Controller {worker_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::Init { result_tx };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub async fn set_colors(
        &self,
        worker_id: u32,
        colors: Arc<ColorBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Controller {worker_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetColors {
            data: colors,
            controller_entry,
            result_tx,
        };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub async fn set_speeds(
        &self,
        worker_id: u32,
        speeds: Arc<SpeedBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        let worker_tx = self
            .workers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Controller {worker_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetSpeeds {
            data: speeds,
            controller_entry,
            result_tx,
        };

        worker_tx.send(command).await?;

        result_rx.await?
    }

    pub fn try_set_colors(
        &self,
        worker_id: u32,
        colors: Arc<ColorBufferData>,
        controller_entry: ControllerEntry,
    ) -> bool {
        if let Some(worker_tx) = self.workers.get(&worker_id) {
            let (result_tx, _) = oneshot::channel();
            let command = ControllerCommand::SetColors {
                data: colors,
                controller_entry,
                result_tx,
            };

            worker_tx.try_send(command).is_ok()
        } else {
            false
        }
    }

    pub fn try_set_speeds(
        &self,
        worker_id: u32,
        speeds: Arc<SpeedBufferData>,
        controller_entry: ControllerEntry,
    ) -> bool {
        if let Some(worker_tx) = self.workers.get(&worker_id) {
            let (result_tx, _) = oneshot::channel();
            let command = ControllerCommand::SetSpeeds {
                data: speeds,
                controller_entry,
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

    pub async fn stop_worker(&mut self, worker_id: u32) -> Result<()> {
        if let Some(worker_tx) = self.workers.get(&worker_id) {
            let (result_tx, result_rx) = oneshot::channel();
            let command = ControllerCommand::Stop { result_tx };

            worker_tx.send(command).await?;

            info!("Stopped worker for controller {}", worker_id);
            result_rx.await?
        } else {
            Err(anyhow!("Controller {worker_id} not found"))
        }
    }

    pub async fn start_worker(&mut self, controller: Arc<dyn FanController>) -> Result<u32> {
        let controller_id = self.next_worker_id;
        self.next_worker_id += 1;

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

        info!("Initialized {} controller workers", self.workers.len());
        Ok(controller_id)
    }

    pub async fn get_firmware(&self, worker_id: u32) -> Result<(u8, u8, u8)> {
        let worker_tx = self
            .workers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Controller {worker_id} not found"))?;

        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::GetFirmware { result_tx };

        worker_tx.send(command).await?;

        result_rx.await?
    }
}
