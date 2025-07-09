//! Event-driven communication system for inter-service messaging.

use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::broadcast;

/// Type of configuration change detected
#[derive(Debug, Clone)]
pub enum ConfigChangeType {
    /// Configuration changes that can be applied without restart
    HotReload,
    /// Configuration changes that require full daemon restart
    ColdRestart {
        /// List of changed hardware-related sections
        changed_sections: Vec<String>,
    },
}

/// Application events for inter-service communication.
///
/// Events are published through the EventBus and consumed by interested services.
/// This enables loose coupling between components.
#[derive(Debug, Clone)]
pub enum Event {
    /// Configuration change detection with type classification
    ConfigChangeDetected(ConfigChangeType),
    SystemShutdown,
    TemperatureChanged(HashMap<String, f32>),
    DeviceConnected {
        /// Device identifier
        vendor_id: u16,
        /// Device type (e.g., "sensor", "actuator")
        product_id: u16,
        /// Optional serial number for the device
        serial_number: Option<String>,
    },
    DeviceDisconnected {
        /// Device identifier
        vendor_id: u16,
        /// Device type (e.g., "sensor", "actuator")
        product_id: u16,
        /// Optional serial number for the device
        serial_number: Option<String>,
    },
    ColorChanged,
}

/// Event bus for publish-subscribe messaging between services.
///
/// Provides a centralized communication mechanism that allows services
/// to communicate without direct dependencies.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::event::{Event, EventBus};
/// use std::collections::HashMap;
///
/// // Create event bus and subscriber
/// let event_bus = EventBus::new();
/// let mut subscriber = event_bus.subscribe();
///
/// // Publish an event
/// let temperatures = HashMap::new();
/// event_bus.publish(Event::TemperatureChanged(temperatures));
///
/// // In async context, receive events:
/// // let event = subscriber.recv().await;
/// ```
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    /// Creates a new EventBus with default capacity.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self { sender }
    }

    /// Creates a new EventBus with custom capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Channel capacity for buffering events
    #[cfg(test)]
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publishes an event to all subscribers.
    ///
    /// Returns an error if there are no active subscribers.
    pub fn publish(&self, event: Event) -> Result<()> {
        self.sender.send(event)?;
        Ok(())
    }

    /// Creates a new subscriber to receive events.
    ///
    /// Each subscriber receives all events published after subscription.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
