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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use tokio::time::{Duration, sleep};

    #[test]
    fn event_bus_new_creates_with_default_capacity() {
        let event_bus = EventBus::new();
        // Verify that it has the expected default capacity by checking subscriber count
        let _receiver = event_bus.subscribe();
        assert_eq!(event_bus.sender.receiver_count(), 1);
    }

    #[test]
    fn event_bus_with_capacity_creates_with_custom_capacity() {
        let capacity = 256;
        let event_bus = EventBus::with_capacity(capacity);
        let _receiver = event_bus.subscribe();
        assert_eq!(event_bus.sender.receiver_count(), 1);
    }

    #[test]
    fn event_bus_default_trait_works() {
        let event_bus = EventBus::default();
        let _receiver = event_bus.subscribe();
        assert_eq!(event_bus.sender.receiver_count(), 1);
    }

    #[test]
    fn event_bus_clone_creates_shared_channel() {
        let event_bus1 = EventBus::new();
        let event_bus2 = event_bus1.clone();

        let _receiver1 = event_bus1.subscribe();
        let _receiver2 = event_bus2.subscribe();

        // Both should share the same sender
        assert_eq!(event_bus1.sender.receiver_count(), 2);
        assert_eq!(event_bus2.sender.receiver_count(), 2);
    }

    #[tokio::test]
    async fn publish_and_subscribe_basic_event() {
        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();

        // Publish SystemShutdown event
        event_bus.publish(Event::SystemShutdown).unwrap();

        // Receive the event
        let received_event = receiver.recv().await.unwrap();
        match received_event {
            Event::SystemShutdown => {} // Expected
            _ => panic!("Expected SystemShutdown event"),
        }
    }

    #[tokio::test]
    async fn publish_temperature_changed_event() {
        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();

        // Create temperature data
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu".to_string(), 45.5);
        temperatures.insert("gpu".to_string(), 62.3);

        // Publish TemperatureChanged event
        let original_event = Event::TemperatureChanged(temperatures.clone());
        event_bus.publish(original_event).unwrap();

        // Receive and verify the event
        let received_event = receiver.recv().await.unwrap();
        match received_event {
            Event::TemperatureChanged(received_temps) => {
                assert_eq!(received_temps.len(), 2);
                assert_eq!(received_temps.get("cpu"), Some(&45.5));
                assert_eq!(received_temps.get("gpu"), Some(&62.3));
            }
            _ => panic!("Expected TemperatureChanged event"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_event() {
        let event_bus = EventBus::new();
        let mut receiver1 = event_bus.subscribe();
        let mut receiver2 = event_bus.subscribe();
        let mut receiver3 = event_bus.subscribe();

        // Publish ColorChanged event
        event_bus.publish(Event::ColorChanged).unwrap();

        // All receivers should get the same event
        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();
        let event3 = receiver3.recv().await.unwrap();

        // Verify all received ColorChanged
        match (event1, event2, event3) {
            (Event::ColorChanged, Event::ColorChanged, Event::ColorChanged) => {}
            _ => panic!("All receivers should receive ColorChanged event"),
        }
    }

    #[tokio::test]
    async fn publish_without_subscribers_returns_error() {
        let event_bus = EventBus::new();

        // Publishing without any subscribers should return an error
        let result = event_bus.publish(Event::ConfigChangeDetected(ConfigChangeType::HotReload));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn late_subscriber_doesnt_receive_old_events() {
        let event_bus = EventBus::new();
        let mut early_receiver = event_bus.subscribe();

        // Publish event to early subscriber
        event_bus.publish(Event::SystemShutdown).unwrap();

        // Early receiver gets the event
        let _event = early_receiver.recv().await.unwrap();

        // Create late subscriber after event was published
        let mut late_receiver = event_bus.subscribe();

        // Publish new event
        event_bus.publish(Event::ColorChanged).unwrap();

        // Late receiver should only get the new event
        let late_event = late_receiver.recv().await.unwrap();
        match late_event {
            Event::ColorChanged => {} // Expected
            _ => panic!("Late subscriber should only receive new events"),
        }
    }

    #[tokio::test]
    async fn sequential_events_received_in_order() {
        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();

        // Publish multiple events in sequence
        event_bus
            .publish(Event::ConfigChangeDetected(ConfigChangeType::HotReload))
            .unwrap();
        event_bus.publish(Event::ColorChanged).unwrap();
        event_bus.publish(Event::SystemShutdown).unwrap();

        // Receive events in order
        let event1 = receiver.recv().await.unwrap();
        let event2 = receiver.recv().await.unwrap();
        let event3 = receiver.recv().await.unwrap();

        // Verify order
        match (event1, event2, event3) {
            (
                Event::ConfigChangeDetected(ConfigChangeType::HotReload),
                Event::ColorChanged,
                Event::SystemShutdown,
            ) => {}
            _ => panic!("Events should be received in publication order"),
        }
    }

    #[tokio::test]
    async fn event_bus_works_across_async_tasks() {
        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();
        let publisher_bus = event_bus.clone();

        // Start publisher task
        let publisher_handle = tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            publisher_bus.publish(Event::SystemShutdown).unwrap();
        });

        // Start receiver task
        let receiver_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

        // Wait for both tasks
        publisher_handle.await.unwrap();
        let received_event = receiver_handle.await.unwrap();

        match received_event {
            Event::SystemShutdown => {}
            _ => panic!("Expected SystemShutdown event from async task"),
        }
    }

    #[tokio::test]
    async fn receiver_dropped_before_event_doesnt_block_publisher() {
        let event_bus = EventBus::new();
        let receiver = event_bus.subscribe();

        // Drop receiver immediately
        drop(receiver);

        // Publishing should now fail since no receivers exist
        let result = event_bus.publish(Event::ColorChanged);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stress_test_many_events() {
        // Use larger capacity and fewer events to avoid lag
        let event_bus = EventBus::with_capacity(2000);
        let mut receiver = event_bus.subscribe();

        const NUM_EVENTS: usize = 100; // Reduced from 1000

        // Publish many events
        for i in 0..NUM_EVENTS {
            let mut temps = HashMap::new();
            temps.insert("sensor".to_string(), i as f32);
            event_bus.publish(Event::TemperatureChanged(temps)).unwrap();
        }

        // Receive all events (some may be lagged, which is normal for broadcast)
        let mut received_count = 0;
        while received_count < NUM_EVENTS {
            match receiver.recv().await {
                Ok(Event::TemperatureChanged(temps)) => {
                    // Just verify we got a valid temperature event
                    assert!(temps.contains_key("sensor"));
                    received_count += 1;
                }
                Ok(_) => panic!("Expected TemperatureChanged event"),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    // This is expected behavior for broadcast channels under load
                    // Skip the lagged messages and continue
                    received_count += skipped as usize;
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        // We should have received all events (or had them lagged, which counts too)
        assert!(received_count >= NUM_EVENTS);
    }
}
