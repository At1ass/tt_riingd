//! Tests for MessageBroker Arc<DashMap> sharing fix
//!
//! Verifies that cloned MessageBroker instances share the same service handlers
//! and that handlers registered on one instance are visible on all clones.

use crate::core::event::{MessageBroker, ServiceType, Request};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_message_broker_clone_shares_handlers() {
    let broker1 = MessageBroker::new();
    let broker2 = broker1.clone();
    
    // Register handler on first broker
    let (tx, _rx) = mpsc::channel::<Request>(10);
    broker1.register_handler(ServiceType::FanColor, tx);
    
    // Verify both brokers see the handler
    assert_eq!(broker1.handler_count(), 1);
    assert_eq!(broker2.handler_count(), 1);
    assert!(broker1.has_handler(ServiceType::FanColor));
    assert!(broker2.has_handler(ServiceType::FanColor));
}

#[tokio::test]
async fn test_message_broker_shared_registration() {
    let broker1 = MessageBroker::new();
    let broker2 = broker1.clone();
    let broker3 = broker2.clone();
    
    // Register handlers on different broker instances
    let (tx1, _rx1) = mpsc::channel::<Request>(10);
    let (tx2, _rx2) = mpsc::channel::<Request>(10);
    
    broker1.register_handler(ServiceType::FanColor, tx1);
    broker2.register_handler(ServiceType::Monitoring, tx2);
    
    // All brokers should see both handlers
    assert_eq!(broker1.handler_count(), 2);
    assert_eq!(broker2.handler_count(), 2); 
    assert_eq!(broker3.handler_count(), 2);
    
    assert!(broker1.has_handler(ServiceType::FanColor));
    assert!(broker1.has_handler(ServiceType::Monitoring));
    assert!(broker2.has_handler(ServiceType::FanColor));
    assert!(broker2.has_handler(ServiceType::Monitoring));
    assert!(broker3.has_handler(ServiceType::FanColor));
    assert!(broker3.has_handler(ServiceType::Monitoring));
}

#[tokio::test]
async fn test_message_broker_handler_replacement() {
    let broker1 = MessageBroker::new();
    let broker2 = broker1.clone();
    
    // Register initial handler
    let (tx1, _rx1) = mpsc::channel::<Request>(10);
    broker1.register_handler(ServiceType::FanColor, tx1);
    assert_eq!(broker1.handler_count(), 1);
    assert_eq!(broker2.handler_count(), 1);
    
    // Replace with new handler on different broker instance
    let (tx2, _rx2) = mpsc::channel::<Request>(10);
    broker2.register_handler(ServiceType::FanColor, tx2);
    
    // Should still have only 1 handler (replaced, not added)
    assert_eq!(broker1.handler_count(), 1);
    assert_eq!(broker2.handler_count(), 1);
    assert!(broker1.has_handler(ServiceType::FanColor));
    assert!(broker2.has_handler(ServiceType::FanColor));
}

#[tokio::test]
async fn test_concurrent_handler_registration() {
    let broker = MessageBroker::new();
    let handles = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    
    // Spawn multiple tasks that register handlers concurrently
    let mut join_handles = Vec::new();
    
    for i in 0..10 {
        let broker_clone = broker.clone();
        let service_type = match i % 4 {
            0 => ServiceType::FanColor,
            1 => ServiceType::Monitoring,
            2 => ServiceType::Broadcast,
            _ => ServiceType::Coordinator,
        };
        
        let handle = tokio::spawn(async move {
            let (tx, _rx) = mpsc::channel::<Request>(10);
            broker_clone.register_handler(service_type, tx);
        });
        
        join_handles.push(handle);
    }
    
    // Wait for all registrations to complete
    for handle in join_handles {
        handle.await.unwrap();
    }
    
    // Should have 4 unique handlers (one for each ServiceType)
    assert_eq!(broker.handler_count(), 4);
    assert!(broker.has_handler(ServiceType::FanColor));
    assert!(broker.has_handler(ServiceType::Monitoring));
    assert!(broker.has_handler(ServiceType::Broadcast));
    assert!(broker.has_handler(ServiceType::Coordinator));
}