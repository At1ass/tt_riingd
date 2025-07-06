# REFACTORING.md

Полная инструкция по рефакторингу tt_riingd для перехода на новую архитектуру с унифицированным ControllerManager и Command Pattern.

## 📋 Содержание

1. [Обзор рефакторинга](#обзор-рефакторинга)
2. [Архитектурные изменения](#архитектурные-изменения) 
3. [План миграции](#план-миграции)
4. [Детальные инструкции](#детальные-инструкции)
5. [Тестирование](#тестирование)
6. [Откат изменений](#откат-изменений)

## 🎯 Обзор рефакторинга

### Цели рефакторинга

- **Производительность**: Устранение spawn overhead (от 240 spawns/sec к 0)
- **Унификация**: Единый API для всех операций с контроллерами
- **Type Safety**: Compile-time проверки типов команд
- **Масштабируемость**: Легкое добавление новых типов команд
- **Maintainability**: Четкое разделение ответственности

### Ключевые изменения

```
БЫЛО:                           СТАЛО:
├── spawn tasks каждый кадр     ├── persistent workers
├── множественные клонирования  ├── ссылки до worker boundary  
├── разрозненные методы API     ├── унифицированный batch_update
└── неоптимальная латентность   └── near-optimal performance
```

### Архитектурная схема (финальная)

```
┌─────────────────────────┐
│  ServiceProviders       │ ← Unchanged API
│  - MonitoringService    │
│  - FanColorService      │
└─────────┬───────────────┘
          │ &references (до границы)
┌─────────▼───────────────┐
│  ControllerManager      │ ← Фасад с Command Pattern
│  (Facade + Dispatcher)  │
└─────────┬───────────────┘
          │ BatchCommand
┌─────────▼───────────────┐
│ ControllerWorkerManager │ ← Управление worker pool
└─────────┬───────────────┘
          │ mpsc channels
┌─────────▼───────────────┐
│ ControllerWorker[1..N]  │ ← Persistent workers
└─────────┬───────────────┘
          │ clone + move (единственная граница)
┌─────────▼───────────────┐
│ spawn_blocking          │ ← Async/Sync boundary
└─────────┬───────────────┘
          │ sync calls
┌─────────▼───────────────┐
│ FanController impls     │ ← Simple, no internal tasks
│ (TTRiingQuad)           │
└─────────┬───────────────┘
          │ HID API
┌─────────▼───────────────┐
│ Hardware                │
└─────────────────────────┘
```

## 🔄 Архитектурные изменения

### Принцип "Reference Until Blocking Boundary"

```rust
// Вся цепочка работает со ссылками до spawn_blocking
ServiceProvider (ссылки)
    ↓ &ControllerColorBuffer
ControllerManager (ссылки) 
    ↓ &[(usize, &[(u8, u8, u8)])]
ControllerWorker (ссылки)
    ↓ Clone + Move (ЕДИНСТВЕННОЕ копирование)
spawn_blocking (owned data)
    ↓ HID API (sync)
```

### Новые компоненты

1. **BatchCommand** - Type-safe команды
2. **ControllerWorkerManager** - Управление worker pool
3. **ControllerWorker** - Persistent workers для каждого контроллера
4. **ExecutionMode** - Стратегии выполнения (Blocking/FireAndForget/BestEffort)

## 📅 План миграции

### Фаза 1: Подготовка (1-2 дня)
- [ ] Создание новых структур данных
- [ ] Реализация ControllerWorkerManager
- [ ] Обновление FanController trait

### Фаза 2: Core рефакторинг (2-3 дня)
- [ ] Реализация Command Pattern
- [ ] Рефакторинг ControllerManager
- [ ] Обновление TTRiingQuad

### Фаза 3: Service Providers (1 день)
- [ ] Обновление FanColorServiceProvider
- [ ] Обновление MonitoringServiceProvider

### Фаза 4: Тестирование (1-2 дня)
- [ ] Unit tests для новых компонентов
- [ ] Integration tests
- [ ] Performance benchmarks

### Фаза 5: Финализация (1 день)
- [ ] Документация
- [ ] Cleanup старого кода
- [ ] Release preparation

## 🔧 Детальные инструкции

### Шаг 1: Создание новых структур данных

Создайте файл `src/controllers/commands.rs`:

```rust
use std::collections::HashMap;

// Type-safe команды для контроллеров
#[derive(Debug)]
pub enum BatchCommand<'a> {
    SetAllColors {
        data: &'a HashMap<u8, ControllerColorBuffer>,
    },
    SetAllSpeeds {
        data: &'a HashMap<u8, Vec<(usize, f32, u8)>>,
    },
    InitAllControllers {
        controller_ids: &'a [u8],
    },
    GetAllFirmware {
        controller_ids: &'a [u8],
    },
}

// Результаты выполнения команд
#[derive(Debug)]
pub enum BatchResult {
    ColorsSet(ControllerBatchStats),
    SpeedsSet(ControllerBatchStats),
    ControllersInitialized(ControllerBatchStats),
    FirmwareRetrieved {
        stats: ControllerBatchStats,
        firmware_data: HashMap<u8, (u8, u8, u8)>,
    },
}

#[derive(Debug)]
pub struct ControllerBatchStats {
    pub total: usize,
    pub successful: usize,
    pub failed: usize,
    pub failed_controllers: Vec<u8>,
}

// Режимы выполнения команд
#[derive(Debug, Clone)]
pub enum ExecutionMode {
    Blocking,           // Ждем завершения всех операций
    FireAndForget,      // Отправляем и забываем
    BestEffort,         // Продолжаем выполнение при ошибках
}
```

### Шаг 2: Создание ControllerWorkerManager

Создайте файл `src/controllers/worker_manager.rs`:

```rust
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};
use anyhow::Result;

use crate::drivers::fan_controller::FanController;

// Команды для individual worker'а
#[derive(Debug)]
enum ControllerCommand {
    SetSpeeds {
        data: Vec<(usize, f32, u8)>,
        result_tx: oneshot::Sender<Result<(), String>>,
    },
    SetColors {
        data: Vec<(usize, Vec<(u8, u8, u8)>)>,
        result_tx: oneshot::Sender<Result<(), String>>,
    },
    Init {
        result_tx: oneshot::Sender<Result<(), String>>,
    },
    GetFirmware {
        result_tx: oneshot::Sender<Result<(u8, u8, u8), String>>,
    },
}

// Worker для одного контроллера
struct ControllerWorker {
    controller_id: u8,
    controller: Arc<dyn FanController>,
    command_rx: mpsc::Receiver<ControllerCommand>,
}

impl ControllerWorker {
    async fn run(mut self) {
        info!("Controller worker {} starting", self.controller_id);
        
        while let Some(command) = self.command_rx.recv().await {
            match command {
                ControllerCommand::SetSpeeds { data, result_tx } => {
                    let result = self.handle_set_speeds(data).await;
                    let _ = result_tx.send(result);
                }
                ControllerCommand::SetColors { data, result_tx } => {
                    let result = self.handle_set_colors(data).await;
                    let _ = result_tx.send(result);
                }
                ControllerCommand::Init { result_tx } => {
                    let result = self.handle_init().await;
                    let _ = result_tx.send(result);
                }
                ControllerCommand::GetFirmware { result_tx } => {
                    let result = self.handle_get_firmware().await;
                    let _ = result_tx.send(result);
                }
            }
        }
        
        info!("Controller worker {} stopped", self.controller_id);
    }
    
    async fn handle_set_speeds(&self, data: Vec<(usize, f32, u8)>) -> Result<(), String> {
        self.controller
            .update_speed_batch(&data)
            .await
            .map_err(|e| e.to_string())
    }
    
    async fn handle_set_colors(&self, data: Vec<(usize, Vec<(u8, u8, u8)>)>) -> Result<(), String> {
        let batch_refs: Vec<(usize, &[(u8, u8, u8)])> = data
            .iter()
            .map(|(ch, colors)| (*ch, colors.as_slice()))
            .collect();
            
        self.controller
            .update_color_batch(&batch_refs)
            .await
            .map_err(|e| e.to_string())
    }
    
    async fn handle_init(&self) -> Result<(), String> {
        self.controller
            .send_init()
            .await
            .map_err(|e| e.to_string())
    }
    
    async fn handle_get_firmware(&self) -> Result<(u8, u8, u8), String> {
        self.controller
            .firmware_version()
            .await
            .map_err(|e| e.to_string())
    }
}

// Менеджер всех Controller Workers
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
    
    // Blocking API для гарантированной доставки
    pub async fn set_colors(
        &self,
        controller_id: u8,
        colors: &[(usize, &[(u8, u8, u8)])],
    ) -> Result<(), String> {
        let worker_tx = self.workers.get(&controller_id)
            .ok_or_else(|| format!("Controller {} not found", controller_id))?;
            
        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetColors {
            data: colors.iter()
                .map(|(ch, rgb_slice)| (*ch, rgb_slice.to_vec()))
                .collect(),
            result_tx,
        };
        
        worker_tx.send(command).await
            .map_err(|_| "Worker disconnected".to_string())?;
            
        result_rx.await
            .map_err(|_| "Worker result lost".to_string())?
    }
    
    pub async fn set_speeds(
        &self,
        controller_id: u8,
        speeds: &[(usize, f32, u8)],
    ) -> Result<(), String> {
        let worker_tx = self.workers.get(&controller_id)
            .ok_or_else(|| format!("Controller {} not found", controller_id))?;
            
        let (result_tx, result_rx) = oneshot::channel();
        let command = ControllerCommand::SetSpeeds {
            data: speeds.to_vec(),
            result_tx,
        };
        
        worker_tx.send(command).await
            .map_err(|_| "Worker disconnected".to_string())?;
            
        result_rx.await
            .map_err(|_| "Worker result lost".to_string())?
    }
    
    // Non-blocking API для максимальной производительности
    pub fn try_set_colors(&self, controller_id: u8, colors: &[(usize, &[(u8, u8, u8)])]) -> bool {
        if let Some(worker_tx) = self.workers.get(&controller_id) {
            let (result_tx, _) = oneshot::channel();
            let command = ControllerCommand::SetColors {
                data: colors.iter()
                    .map(|(ch, rgb_slice)| (*ch, rgb_slice.to_vec()))
                    .collect(),
                result_tx,
            };
            
            worker_tx.try_send(command).is_ok()
        } else {
            false
        }
    }
    
    pub fn try_set_speeds(&self, controller_id: u8, speeds: &[(usize, f32, u8)]) -> bool {
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
}
```

### Шаг 3: Обновление FanController trait

В файле `src/drivers/fan_controller.rs`, обновите trait для использования ссылок:

```rust
#[async_trait]
pub trait FanController: Send + Sync + Debug {
    async fn send_init(&self) -> Result<()>;
    
    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()>;
    
    // Обновлено: принимает ссылки вместо owned data
    async fn update_speed_batch(&self, batch: &[(usize, f32, u8)]) -> Result<()>;
    
    async fn update_channel_color(&self, channel: u8, red: u8, green: u8, blue: u8) -> Result<()>;
    
    // Обновлено: принимает ссылки вместо owned data
    async fn update_color_batch(&self, batch: &[(usize, &[(u8, u8, u8)])]) -> Result<()>;
    
    async fn firmware_version(&self) -> Result<(u8, u8, u8)>;
    
    fn led_count(&self) -> usize;
    
    fn hardware_info() -> HardwareInfo where Self: Sized;
}
```

### Шаг 4: Рефакторинг ControllerManager

В файле `src/drivers/controller_manager.rs`:

```rust
use std::sync::Arc;
use anyhow::Result;
use tracing::warn;

use crate::config::Config;
use crate::drivers::fan_controller::FanController;
use super::commands::{BatchCommand, BatchResult, ExecutionMode, ControllerBatchStats};
use super::worker_manager::ControllerWorkerManager;

pub struct ControllerManager {
    worker_manager: ControllerWorkerManager,
}

impl ControllerManager {
    pub fn init_from_cfg(cfg: &Config) -> Result<Self> {
        let mut controllers = Vec::<Arc<dyn FanController>>::new();
        
        // Создаем контроллеры как обычно
        match super::HIDAPI.as_ref() {
            Some(hidapi) => {
                controllers.extend(
                    crate::drivers::tt_riing_quad::TTRiingQuad::find_controllers(hidapi, &cfg.controllers)?
                        .into_iter()
                        .map(|c| Arc::new(c) as Arc<dyn FanController>)
                );
            }
            None => {
                warn!("HID API not available, no hardware controllers will be initialized");
            }
        }
        
        // Инициализируем worker manager
        let mut worker_manager = ControllerWorkerManager::new();
        worker_manager.initialize(controllers).await?;
        
        Ok(Self { worker_manager })
    }
    
    // Унифицированный API с Command Pattern
    pub async fn batch_update(
        &self,
        command: BatchCommand<'_>,
        mode: ExecutionMode,
    ) -> Result<BatchResult> {
        match command {
            BatchCommand::SetAllColors { data } => {
                let stats = self.handle_set_all_colors(data, mode).await?;
                Ok(BatchResult::ColorsSet(stats))
            }
            BatchCommand::SetAllSpeeds { data } => {
                let stats = self.handle_set_all_speeds(data, mode).await?;
                Ok(BatchResult::SpeedsSet(stats))
            }
            BatchCommand::InitAllControllers { controller_ids } => {
                let stats = self.handle_init_all_controllers(controller_ids, mode).await?;
                Ok(BatchResult::ControllersInitialized(stats))
            }
            BatchCommand::GetAllFirmware { controller_ids } => {
                let (stats, firmware_data) = self.handle_get_all_firmware(controller_ids, mode).await?;
                Ok(BatchResult::FirmwareRetrieved { stats, firmware_data })
            }
        }
    }
    
    // Специализированные обработчики для каждого типа команд
    async fn handle_set_all_colors(
        &self,
        color_data: &HashMap<u8, ControllerColorBuffer>,
        mode: ExecutionMode,
    ) -> Result<ControllerBatchStats> {
        match mode {
            ExecutionMode::Blocking => self.set_colors_blocking(color_data).await,
            ExecutionMode::FireAndForget => Ok(self.set_colors_fire_and_forget(color_data)),
            ExecutionMode::BestEffort => self.set_colors_best_effort(color_data).await,
        }
    }
    
    async fn set_colors_blocking(
        &self,
        color_data: &HashMap<u8, ControllerColorBuffer>,
    ) -> Result<ControllerBatchStats> {
        let total = color_data.len();
        let mut successful = 0;
        let mut failed_controllers = Vec::new();
        
        // Параллельно отправляем всем контроллерам
        let futures: Vec<_> = color_data
            .iter()
            .map(|(controller_id, buffer)| {
                let batch_refs: Vec<(usize, &[(u8, u8, u8)])> = buffer
                    .iter()
                    .map(|(channel, colors)| (*channel, colors.as_slice()))
                    .collect();
                
                async move {
                    let result = self.worker_manager
                        .set_colors(*controller_id, &batch_refs)
                        .await;
                    (*controller_id, result)
                }
            })
            .collect();
        
        let results = futures::future::join_all(futures).await;
        
        for (controller_id, result) in results {
            match result {
                Ok(_) => successful += 1,
                Err(e) => {
                    error!("Controller {} color update failed: {}", controller_id, e);
                    failed_controllers.push(controller_id);
                }
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
            let batch_refs: Vec<(usize, &[(u8, u8, u8)])> = buffer
                .iter()
                .map(|(channel, colors)| (*channel, colors.as_slice()))
                .collect();
            
            if self.worker_manager.try_set_colors(*controller_id, &batch_refs) {
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
    
    // Аналогично для других типов команд...
    // handle_set_all_speeds(), handle_init_all_controllers(), etc.
}

// Backward compatibility - сохраняем старые методы
impl ControllerManager {
    pub async fn update_channel_color_batch(
        &self,
        controller: u8,
        batch: &ControllerColorBuffer,
    ) -> Result<()> {
        let mut color_data = HashMap::new();
        color_data.insert(controller, batch.clone());
        
        let command = BatchCommand::SetAllColors { data: &color_data };
        self.batch_update(command, ExecutionMode::Blocking).await?;
        
        Ok(())
    }
    
    pub async fn update_channel_batch(
        &self,
        controller: u8,
        batch: Vec<(usize, f32, u8)>,
    ) -> Result<()> {
        let mut speed_data = HashMap::new();
        speed_data.insert(controller, batch);
        
        let command = BatchCommand::SetAllSpeeds { data: &speed_data };
        self.batch_update(command, ExecutionMode::Blocking).await?;
        
        Ok(())
    }
}
```

### Шаг 5: Обновление TTRiingQuad

В файле `src/drivers/tt_riing_quad/ttriing_quad.rs`, обновите реализацию FanController:

```rust
#[async_trait]
impl FanController for TTRiingQuad {
    // Обновленная реализация с ссылками
    async fn update_color_batch(&self, batch: &[(usize, &[(u8, u8, u8)])]) -> Result<()> {
        debug!("Batch processing {} channels", batch.len());
        let ctrl = self.0.clone();
        
        // Клонируем данные ТОЛЬКО для пересечения async/sync границы
        let batch_owned: Vec<(usize, Vec<(u8, u8, u8)>)> = batch
            .iter()
            .map(|(idx, colors)| (*idx, colors.to_vec()))
            .collect();
            
        // spawn_blocking для синхронного HID API
        tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            
            batch_owned.into_iter().try_fold((), |_, (idx, buffer)| {
                Self::proccess_fan_inner_color(&guard, idx, buffer)
                    .map_err(|e| anyhow::anyhow!("Failed to set color for fan {}: {}", idx, e))
            })
        })
        .await?
    }
    
    async fn update_speed_batch(&self, batch: &[(usize, f32, u8)]) -> Result<()> {
        debug!("Batch processing {} speed updates", batch.len());
        let ctrl = self.0.clone();
        
        // Клонируем для spawn_blocking
        let batch_owned = batch.to_vec();
        
        let result = tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            batch_owned
                .into_iter()
                .map(|(idx, _temp, speed)| {
                    Self::proccess_fan_inner(&guard, idx, speed)
                        .map(|(speed, rpm)| (idx, speed, rpm))
                })
                .collect::<Result<Vec<_>>>()
        })
        .await??;

        let mut guard = self.0.lock().await;
        result.into_iter().for_each(|(channel, speed, rpm)| {
            guard.fans[channel - 1].update_stats(speed, rpm);
        });
        
        Ok(())
    }
}
```

### Шаг 6: Обновление Service Providers

#### FanColorServiceProvider

В файле `src/providers/fan_color.rs`:

```rust
use super::commands::{BatchCommand, ExecutionMode};

async fn transmit_color_changes(
    state: Arc<AppState>,
    buffer: Arc<DoubleBuffer>,
) -> Result<()> {
    let read_buffer = buffer.get_read_buffer().await;
    
    let command = BatchCommand::SetAllColors {
        data: &read_buffer,
    };
    
    let result = state
        .controllers
        .read()
        .await
        .batch_update(command, ExecutionMode::FireAndForget)
        .await?;
    
    if let BatchResult::ColorsSet(stats) = result {
        if stats.failed > 0 {
            warn!("Failed to update colors on {} controllers: {:?}", 
                  stats.failed, stats.failed_controllers);
        }
    }
    
    Ok(())
}
```

#### MonitoringServiceProvider

В файле `src/providers/monitoring.rs`:

```rust
use super::commands::{BatchCommand, ExecutionMode};

async fn collect_and_process_temperatures(
    state: &Arc<AppState>,
    event_bus: &EventBus,
) -> Result<()> {
    let mut temperatures = HashMap::new();
    let mut batch_data: HashMap<u8, Vec<(usize, f32, u8)>> = HashMap::new();
    
    // Сбор температур и расчет скоростей (без изменений)
    // ...
    
    // Отправляем все данные через унифицированный API
    let command = BatchCommand::SetAllSpeeds {
        data: &batch_data,
    };
    
    let result = state
        .controllers
        .read()
        .await
        .batch_update(command, ExecutionMode::BestEffort)
        .await?;
    
    if let BatchResult::SpeedsSet(stats) = result {
        if stats.failed > 0 {
            warn!("Failed to update speeds on {} controllers: {:?}", 
                  stats.failed, stats.failed_controllers);
        }
        info!("Updated speeds on {}/{} controllers", stats.successful, stats.total);
    }
    
    // Публикация events (без изменений)
    *state.sensor_data.write().await = temperatures.clone();
    if let Err(e) = event_bus.publish(Event::TemperatureChanged(temperatures)) {
        error!("Failed to publish temperature event: {e}");
    }
    
    Ok(())
}
```

### Шаг 7: Обновление модульной структуры

Обновите `src/drivers/mod.rs`:

```rust
pub mod fan_controller;
pub mod controller_manager;
pub mod commands;           // Новый модуль
pub mod worker_manager;     // Новый модуль
pub mod tt_riing_quad;

// Re-exports
pub use controller_manager::ControllerManager;
pub use commands::{BatchCommand, BatchResult, ExecutionMode};
```

## 🧪 Тестирование

### Unit Tests

Создайте файл `src/drivers/tests/controller_manager_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_batch_colors_blocking() {
        let manager = setup_test_manager().await;
        let mut color_data = HashMap::new();
        
        // Подготовка тестовых данных
        let test_buffer = vec![
            (1, vec![(255, 0, 0); 12]),  // Красный цвет для канала 1
            (2, vec![(0, 255, 0); 12]), // Зеленый цвет для канала 2
        ];
        color_data.insert(1, test_buffer);
        
        let command = BatchCommand::SetAllColors { data: &color_data };
        let result = manager.batch_update(command, ExecutionMode::Blocking).await;
        
        assert!(result.is_ok());
        if let Ok(BatchResult::ColorsSet(stats)) = result {
            assert_eq!(stats.successful, 1);
            assert_eq!(stats.failed, 0);
        }
    }
    
    #[tokio::test]
    async fn test_batch_speeds_fire_and_forget() {
        let manager = setup_test_manager().await;
        let mut speed_data = HashMap::new();
        
        let test_speeds = vec![
            (1, 45.0, 75),  // Канал 1: 45°C -> 75% скорость
            (2, 50.0, 80),  // Канал 2: 50°C -> 80% скорость
        ];
        speed_data.insert(1, test_speeds);
        
        let command = BatchCommand::SetAllSpeeds { data: &speed_data };
        let result = manager.batch_update(command, ExecutionMode::FireAndForget).await;
        
        assert!(result.is_ok());
        if let Ok(BatchResult::SpeedsSet(stats)) = result {
            assert_eq!(stats.total, 1);
            // Fire-and-forget может не гарантировать успех
        }
    }
    
    async fn setup_test_manager() -> ControllerManager {
        // Инициализация с mock контроллерами
        // ...
    }
}
```

### Integration Tests

Создайте файл `tests/integration_new_architecture.rs`:

```rust
use tt_riingd::drivers::{ControllerManager, BatchCommand, ExecutionMode};
use std::collections::HashMap;

#[tokio::test]
async fn test_end_to_end_color_pipeline() {
    // Тест полного pipeline: ServiceProvider -> ControllerManager -> Workers -> HID
    // ...
}

#[tokio::test] 
async fn test_performance_benchmark() {
    // Benchmark: старая vs новая архитектура
    // Измеряем latency и throughput
    // ...
}
```

### Performance Tests

```rust
#[tokio::test]
async fn benchmark_spawn_vs_workers() {
    let iterations = 1000;
    
    // Тест старой архитектуры (spawn каждый раз)
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        tokio::spawn(async {
            // Simulated work
            tokio::time::sleep(Duration::from_micros(100)).await;
        }).await.unwrap();
    }
    let old_duration = start.elapsed();
    
    // Тест новой архитектуры (workers)
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        // Send to worker via channel
        // ...
    }
    let new_duration = start.elapsed();
    
    println!("Old: {:?}, New: {:?}, Improvement: {:.2}x", 
             old_duration, new_duration, 
             old_duration.as_nanos() as f64 / new_duration.as_nanos() as f64);
}
```

## 🔄 Откат изменений

### Стратегия отката

1. **Feature flag подход**: Добавьте feature flag для переключения между архитектурами
2. **Backward compatibility**: Сохраняйте старые методы как deprecated
3. **Gradual migration**: Мигрируйте сервисы по одному

### Конфигурационный откат

В `Cargo.toml`:

```toml
[features]
default = ["new-architecture"]
new-architecture = []
legacy-architecture = []
```

В коде:

```rust
#[cfg(feature = "new-architecture")]
pub use new_controller_manager::ControllerManager;

#[cfg(feature = "legacy-architecture")]
pub use legacy_controller_manager::ControllerManager;
```

### Мониторинг критических метрик

```rust
// Добавьте метрики для сравнения производительности
#[derive(Debug)]
pub struct PerformanceMetrics {
    pub latency_p95: Duration,
    pub throughput_rps: f64,
    pub error_rate: f64,
    pub memory_usage: usize,
}

impl ControllerManager {
    pub fn get_metrics(&self) -> PerformanceMetrics {
        // Сбор метрик для monitoring
        // ...
    }
}
```

## 📊 Контрольные точки

### Критерии успеха

- [ ] **Производительность**: Latency < 1ms (было 2-5ms)
- [ ] **Throughput**: 60 FPS без потерь кадров
- [ ] **Memory**: Stable memory usage (no leaks)
- [ ] **Reliability**: Zero crashes в течение 24h stress test
- [ ] **API Compatibility**: Все существующие тесты проходят

### Rollback критерии

- Performance регрессия > 10%
- Crash rate > 0.1% 
- Memory leak > 10MB/hour
- API breaking changes

## 📚 Дополнительные ресурсы

### Документация
- [Tokio Best Practices](https://tokio.rs/tokio/tutorial)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Command Pattern в Rust](https://rust-unofficial.github.io/patterns/patterns/behavioural/command.html)

### Полезные инструменты
- `cargo flamegraph` - профилирование производительности
- `cargo bench` - бенчмарки
- `tokio-console` - отладка async кода
- `valgrind` - проверка memory leaks

---

**Автор**: Claude Code  
**Дата**: 2025-01-04  
**Версия**: 1.0
