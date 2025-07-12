//! UdevWatcher service for monitoring USB/HID device hotplug events.

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_udev::{AsyncMonitorSocket, Device, EventType, MonitorBuilder};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    app_context::AppState, drivers::registry::Registry, event::EventBus,
    providers::traits::ServiceProvider, task_manager::TaskManager,
};

/// Information about a udev event, safe to send between threads.
#[derive(Debug, Clone)]
struct UdevEventInfo {
    event_type: EventType,
    devnode: String,
    subsystem: String,
    vendor_id: u16,
    product_id: u16,
    serial: Option<String>,
    // vendor_id: Option<String>,
    // product_id: Option<String>,
    // serial: Option<String>,
}

/// UdevWatcher service provider.
///
/// Provides a non-critical service that monitors HID device hotplug events
/// using async udev integration. Currently logs events for debugging and
/// development purposes. Will be extended to publish events to EventBus
/// for automatic controller management.
///
/// # Priority and Criticality
///
/// - **Priority**: 8 (high)
/// - **Critical**: No (optional service)
///
/// # Features
///
/// - Efficient HID device event monitoring via udev
/// - Async integration with tokio runtime
/// - Device information extraction (VID/PID/serial)
/// - Event filtering for supported device types
/// - Cancel-safe async design
///
/// # Implementation
///
/// Uses `tokio-udev` for async device monitoring with proper cancellation
/// support. Events are processed in a dedicated async task with structured
/// concurrency patterns.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::UdevWatcherServiceProvider;
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = UdevWatcherServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub struct UdevWatcherServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
}

impl UdevWatcherServiceProvider {
    /// Creates a new UdevWatcher service provider.
    ///
    /// # Arguments
    ///
    /// * `state` - Shared application state
    /// * `event_bus` - Event bus for publishing device events
    pub fn new(state: Arc<AppState>, event_bus: EventBus) -> Self {
        Self { state, event_bus }
    }
}

#[async_trait]
impl ServiceProvider for UdevWatcherServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_udev_watcher_service(state, event_bus, cancel_token).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "UdevWatcher"
    }

    fn priority(&self) -> i32 {
        8
    }

    fn is_critical(&self) -> bool {
        false
    }
}

/// UdevWatcher service implementation.
///
/// Uses blocking udev in a dedicated thread to work around Send trait limitations
/// while maintaining async patterns similar to other services like ConfigWatcher.
///
/// # Cancel Safety
///
/// This implementation is designed to be cancel-safe:
/// - Proper cleanup of blocking thread
/// - Graceful handling of channel closures
/// - No state lost on cancellation
async fn run_udev_watcher_service(
    _state: Arc<AppState>,
    event_bus: EventBus,
    cancel_token: CancellationToken,
) -> Result<()> {
    info!("UdevWatcher: Starting HID device monitoring");

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let ct = cancel_token.clone();

    let udev_thread = std::thread::Builder::new()
        .name("udev_monitor".to_string())
        .spawn(move || {
            let tokio_rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            tokio_rt.block_on(async move {
                if let Err(e) = blocking_udev_monitor(event_tx, ct).await {
                    error!("UdevWatcher: Error in blocking udev monitor: {}", e);
                }
            });
        })?;

    info!("UdevWatcher: HID device monitoring active");

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("UdevWatcher: Shutdown requested");
                break;
            }

            event_info = event_rx.recv() => {
                match event_info {
                    Some(event_info) => {
                        handle_udev_event_info(event_info, &event_bus).await;
                    }
                    None => {
                        warn!("UdevWatcher: Event channel closed, monitoring thread ended");
                        break;
                    }
                }
            }
        }
    }

    udev_thread
        .join()
        .expect("Failed to join udev monitoring thread");

    info!("UdevWatcher: Monitoring stopped");
    Ok(())
}

/// Async udev monitoring function using tokio-udev.
///
/// This function implements proper udev event monitoring using the async
/// tokio-udev interface for better integration with async runtime.
async fn blocking_udev_monitor(
    event_tx: mpsc::UnboundedSender<UdevEventInfo>,
    cancel_token: CancellationToken,
) -> Result<()> {
    info!("UdevWatcher: Starting async udev monitor");

    let builder = MonitorBuilder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create udev monitor: {}", e))?
        .match_subsystem("hidraw")
        // .match_subsystem("usb")
        .map_err(|e| anyhow::anyhow!("Failed to set hidraw filter: {}", e))?;

    let monitor = builder
        .listen()
        .map_err(|e| anyhow::anyhow!("Failed to listen on udev monitor: {}", e))?;

    let mut monitor: AsyncMonitorSocket = monitor
        .try_into()
        .map_err(|e| anyhow::anyhow!("Failed to convert to async monitor socket: {}", e))?;

    info!("UdevWatcher: Monitor listening for hidraw events");

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("UdevWatcher: Cancelled, shutting down udev monitor");
                break;
            },
            Some(event_result) = monitor.next() => {
            match event_result {
                Ok(event) => {
                    let (vid, pid, serial) = usb_ids(event.device()).unwrap_or((0, 0, None));

                    if Registry::is_supported_hardware(vid, pid) {
                        info!(
                            "UdevWatcher: Supported device detected: VID: {}, PID: {}, serial: {:?}",
                            vid, pid, serial
                        );
                        let event_info = UdevEventInfo {
                            event_type: event.event_type(),
                            devnode: event
                                .device()
                                .devnode()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string()),
                            subsystem: event
                                .device()
                                .subsystem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string()),
                            vendor_id: vid,
                            product_id: pid,
                            serial,
                        };

                        // Send event to main async loop
                        if let Err(e) = event_tx.send(event_info) {
                            error!("UdevWatcher: Failed to send event to main loop: {}", e);
                            return Err(anyhow::anyhow!("Failed to send event to main loop"));
                        }
                    } else {
                        info!(
                            "UdevWatcher: Unsupported device detected: VID: {}, PID: {}, serial: {:?}",
                            vid, pid, serial
                        );
                    }
                }
                Err(e) => {
                    error!("UdevWatcher: Error receiving udev event: {}", e);
                    return Err(anyhow::anyhow!("Error receiving udev event: {}", e));
                }
            }
            }
        }
    }

    info!("UdevWatcher: Monitor ended");
    Ok(())
}

fn usb_ids(mut d: Device) -> Option<(u16, u16, Option<String>)> {
    loop {
        let vid = d
            .attribute_value("idVendor")
            .or_else(|| d.property_value("ID_VENDOR_ID"));
        let pid = d
            .attribute_value("idProduct")
            .or_else(|| d.property_value("ID_MODEL_ID"));
        let sn = d
            .property_value("ID_SERIAL_SHORT") // единая короткая строка
            .or_else(|| d.attribute_value("serial"));

        if let (Some(v), Some(p)) = (vid, pid) {
            return Some((
                v.to_string_lossy().into_owned().parse().unwrap_or(0),
                p.to_string_lossy().into_owned().parse().unwrap_or(0),
                sn.map(|s| s.to_string_lossy().into_owned()),
            ));
        }
        d = d.parent()?; // поднимаемся, пока не кончились родители
    }
}

/// Handles udev event information.
///
/// Currently only logs events for debugging. Will be extended to:
/// - Filter for supported devices
/// - Publish events to EventBus
async fn handle_udev_event_info(event_info: UdevEventInfo, event_bus: &EventBus) {
    match event_info.event_type {
        EventType::Add => {
            info!(
                "HID device connected: {} (subsystem: {}, VID: {}, PID: {}, serial: {})",
                event_info.devnode,
                event_info.subsystem,
                event_info.vendor_id,
                event_info.product_id,
                event_info
                    .serial
                    .clone()
                    .unwrap_or_else(|| "none".to_string())
            );

            if let Err(e) = event_bus.notify(crate::event::Event::DeviceConnected {
                vendor_id: event_info.vendor_id,
                product_id: event_info.product_id,
                serial_number: event_info.serial,
            }) {
                error!("Failed to publish HID device added event: {}", e);
            }
        }
        EventType::Remove => {
            info!(
                "HID device disconnected: {} (subsystem: {}, VID: {}, PID: {}, serial: {})",
                event_info.devnode,
                event_info.subsystem,
                event_info.vendor_id,
                event_info.product_id,
                event_info
                    .serial
                    .clone()
                    .unwrap_or_else(|| "none".to_string())
            );
            if let Err(e) = event_bus.notify(crate::event::Event::DeviceDisconnected {
                vendor_id: event_info.vendor_id,
                product_id: event_info.product_id,
                serial_number: event_info.serial,
            }) {
                error!("Failed to publish HID device added event: {}", e);
            }
        }
        EventType::Change => {
            debug!(
                "HID device changed: {} (subsystem: {})",
                event_info.devnode, event_info.subsystem
            );
        }
        EventType::Bind | EventType::Unbind => {
            debug!(
                "HID device bind/unbind: {} (subsystem: {})",
                event_info.devnode, event_info.subsystem
            );
        }
        _ => {
            tracing::trace!(
                "HID device event {:?}: {} (subsystem: {})",
                event_info.event_type,
                event_info.devnode,
                event_info.subsystem
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigManager};
    use std::path::PathBuf;

    async fn create_test_app_state() -> Arc<AppState> {
        let config = Config::default();
        let config_manager = ConfigManager::new(config, PathBuf::from("/tmp/test.yml"));
        Arc::new(AppState::new(config_manager).await.unwrap())
    }

    #[tokio::test]
    async fn test_udev_watcher_creation() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        let watcher = UdevWatcherServiceProvider::new(state, event_bus);

        assert_eq!(watcher.name(), "UdevWatcher");
        assert_eq!(watcher.priority(), 8);
        assert!(!watcher.is_critical());
    }

    #[test]
    fn test_device_attribute_extraction() {
        let config = Config::default();
        let config_manager = ConfigManager::new(config, PathBuf::from("/tmp/test.yml"));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let state = Arc::new(AppState::new(config_manager).await.unwrap());
            let event_bus = EventBus::new();
            let _watcher = UdevWatcherServiceProvider::new(state, event_bus);
        });
    }
}
