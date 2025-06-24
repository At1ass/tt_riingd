//! Unit tests for provider traits

use super::super::traits::*;
use crate::core::TaskManager;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

// Mock AsyncProvider implementations
struct MockSuccessfulProvider<T> {
    value: T,
    call_count: Arc<Mutex<usize>>,
}

impl<T: Clone> MockSuccessfulProvider<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl<T: Clone + Send + Sync> AsyncProvider<T> for MockSuccessfulProvider<T> {
    async fn provide(&self) -> Result<T> {
        *self.call_count.lock().unwrap() += 1;
        Ok(self.value.clone())
    }
}

struct MockFailingProvider {
    error_message: String,
}

impl MockFailingProvider {
    fn new(error_message: &str) -> Self {
        Self {
            error_message: error_message.to_string(),
        }
    }
}

#[async_trait]
impl<T: Send + Sync> AsyncProvider<T> for MockFailingProvider {
    async fn provide(&self) -> Result<T> {
        Err(anyhow!(self.error_message.clone()))
    }
}

struct MockSlowProvider<T> {
    value: T,
    delay_ms: u64,
}

impl<T> MockSlowProvider<T> {
    fn new(value: T, delay_ms: u64) -> Self {
        Self { value, delay_ms }
    }
}

#[async_trait]
impl<T: Clone + Send + Sync> AsyncProvider<T> for MockSlowProvider<T> {
    async fn provide(&self) -> Result<T> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        Ok(self.value.clone())
    }
}

// Mock ServiceProvider implementations
struct MockSuccessfulService {
    name: &'static str,
    priority: i32,
    is_critical: bool,
    start_called: Arc<Mutex<bool>>,
    task_spawned: Arc<Mutex<bool>>,
}

impl MockSuccessfulService {
    fn new(name: &'static str, priority: i32, is_critical: bool) -> Self {
        Self {
            name,
            priority,
            is_critical,
            start_called: Arc::new(Mutex::new(false)),
            task_spawned: Arc::new(Mutex::new(false)),
        }
    }

    fn was_start_called(&self) -> bool {
        *self.start_called.lock().unwrap()
    }

    fn was_task_spawned(&self) -> bool {
        *self.task_spawned.lock().unwrap()
    }
}

#[async_trait]
impl ServiceProvider for MockSuccessfulService {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        *self.start_called.lock().unwrap() = true;

        let task_spawned = self.task_spawned.clone();
        let task_name = format!("{}_task", self.name);

        task_manager
            .spawn_task(task_name, move |_token: CancellationToken| {
                let task_spawned = task_spawned.clone();
                async move {
                    *task_spawned.lock().unwrap() = true;
                    Ok(())
                }
            })
            .await
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn is_critical(&self) -> bool {
        self.is_critical
    }
}

struct MockFailingService {
    name: &'static str,
    error_message: String,
}

impl MockFailingService {
    fn new(name: &'static str, error_message: &str) -> Self {
        Self {
            name,
            error_message: error_message.to_string(),
        }
    }
}

#[async_trait]
impl ServiceProvider for MockFailingService {
    async fn start(&self, _task_manager: &mut TaskManager) -> Result<()> {
        Err(anyhow!("{}: {}", self.name, self.error_message))
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

struct MockSlowService {
    name: &'static str,
    delay_ms: u64,
    inner: MockSuccessfulService,
}

impl MockSlowService {
    fn new(name: &'static str, delay_ms: u64) -> Self {
        Self {
            name,
            delay_ms,
            inner: MockSuccessfulService::new(name, 0, false),
        }
    }
}

#[async_trait]
impl ServiceProvider for MockSlowService {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner.start(task_manager).await
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// Tests for AsyncProvider trait

#[tokio::test]
async fn async_provider_successful_string() {
    let provider = MockSuccessfulProvider::new("test_value".to_string());

    let result = provider.provide().await;
    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), "test_value");
    std::assert_eq!(provider.call_count(), 1);
}

#[tokio::test]
async fn async_provider_successful_integer() {
    let provider = MockSuccessfulProvider::new(42i32);

    let result = provider.provide().await;
    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), 42);
    std::assert_eq!(provider.call_count(), 1);
}

#[tokio::test]
async fn async_provider_successful_complex_type() {
    let test_map = HashMap::from([
        ("key1".to_string(), "value1".to_string()),
        ("key2".to_string(), "value2".to_string()),
    ]);
    let provider = MockSuccessfulProvider::new(test_map.clone());

    let result = provider.provide().await;
    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), test_map);
}

#[tokio::test]
async fn async_provider_multiple_calls() {
    let provider = MockSuccessfulProvider::new("consistent".to_string());

    let result1 = provider.provide().await.unwrap();
    let result2 = provider.provide().await.unwrap();
    let result3 = provider.provide().await.unwrap();

    std::assert_eq!(result1, "consistent");
    std::assert_eq!(result2, "consistent");
    std::assert_eq!(result3, "consistent");
    std::assert_eq!(provider.call_count(), 3);
}

#[tokio::test]
async fn async_provider_failing() {
    let provider: MockFailingProvider = MockFailingProvider::new("Test error");

    let result: Result<String> = provider.provide().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Test error"));
}

#[tokio::test]
async fn async_provider_slow_timing() {
    let provider = MockSlowProvider::new("delayed_value".to_string(), 100);

    let start = std::time::Instant::now();
    let result = provider.provide().await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), "delayed_value");
    assert!(elapsed >= Duration::from_millis(100));
}

#[tokio::test]
async fn async_provider_concurrent_access() {
    let provider = Arc::new(MockSuccessfulProvider::new("shared_value".to_string()));

    let mut handles = Vec::new();
    for i in 0..5 {
        let provider_clone = provider.clone();
        let handle = tokio::spawn(async move {
            let result = provider_clone.provide().await;
            (i, result)
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        let (id, result) = handle.await.unwrap();
        results.push((id, result.unwrap()));
    }

    // All should succeed
    std::assert_eq!(results.len(), 5);
    for (_, value) in results {
        std::assert_eq!(value, "shared_value");
    }

    // Should have been called 5 times
    std::assert_eq!(provider.call_count(), 5);
}

#[tokio::test]
async fn async_provider_trait_object() {
    let provider: Box<dyn AsyncProvider<i32>> = Box::new(MockSuccessfulProvider::new(123));

    let result = provider.provide().await;
    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), 123);
}

// Tests for ServiceProvider trait

#[tokio::test]
async fn service_provider_successful_start() {
    let service = MockSuccessfulService::new("TestService", 1, false);
    let mut task_manager = TaskManager::new();

    let result = service.start(&mut task_manager).await;

    assert!(result.is_ok());
    assert!(service.was_start_called());
    std::assert_eq!(task_manager.active_count(), 1);

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
    assert!(service.was_task_spawned());
}

#[tokio::test]
async fn service_provider_metadata() {
    let service = MockSuccessfulService::new("MetadataService", 42, true);

    std::assert_eq!(service.name(), "MetadataService");
    std::assert_eq!(service.priority(), 42);
    assert!(service.is_critical());
}

#[tokio::test]
async fn service_provider_default_values() {
    struct DefaultService;

    impl DefaultService {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl ServiceProvider for DefaultService {
        async fn start(&self, _task_manager: &mut TaskManager) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> &'static str {
            "DefaultService"
        }
    }

    let service = DefaultService::new();

    std::assert_eq!(service.name(), "DefaultService");
    std::assert_eq!(service.priority(), 0); // Default
    assert!(!service.is_critical()); // Default
}

#[tokio::test]
async fn service_provider_failing_start() {
    let service = MockFailingService::new("FailingService", "Startup error");
    let mut task_manager = TaskManager::new();

    let result = service.start(&mut task_manager).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("FailingService: Startup error")
    );
    std::assert_eq!(task_manager.active_count(), 0);
}

#[tokio::test]
async fn service_provider_slow_start() {
    let service = MockSlowService::new("SlowService", 100);
    let mut task_manager = TaskManager::new();

    let start = std::time::Instant::now();
    let result = service.start(&mut task_manager).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert!(elapsed >= Duration::from_millis(100));
    std::assert_eq!(task_manager.active_count(), 1);

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn service_provider_priority_ordering() {
    let high_priority = MockSuccessfulService::new("HighPriority", 100, false);
    let low_priority = MockSuccessfulService::new("LowPriority", 1, false);

    assert!(high_priority.priority() > low_priority.priority());
    std::assert_eq!(high_priority.priority(), 100);
    std::assert_eq!(low_priority.priority(), 1);
}

#[tokio::test]
async fn service_provider_criticality_classification() {
    let critical = MockSuccessfulService::new("CriticalService", 0, true);
    let optional = MockSuccessfulService::new("OptionalService", 0, false);

    assert!(critical.is_critical());
    assert!(!optional.is_critical());
}

#[tokio::test]
async fn service_provider_trait_object() {
    let service: Box<dyn ServiceProvider> =
        Box::new(MockSuccessfulService::new("BoxedService", 5, true));
    let mut task_manager = TaskManager::new();

    std::assert_eq!(service.name(), "BoxedService");
    std::assert_eq!(service.priority(), 5);
    assert!(service.is_critical());

    let result = service.start(&mut task_manager).await;
    assert!(result.is_ok());

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn service_provider_concurrent_starts() {
    let service1 = Arc::new(MockSuccessfulService::new("Service1", 0, false));
    let service2 = Arc::new(MockSuccessfulService::new("Service2", 0, false));

    let service1_clone = service1.clone();
    let service2_clone = service2.clone();

    let mut task_manager1 = TaskManager::new();
    let mut task_manager2 = TaskManager::new();

    let handle1 = tokio::spawn(async move { service1_clone.start(&mut task_manager1).await });
    let handle2 = tokio::spawn(async move { service2_clone.start(&mut task_manager2).await });

    let (result1, result2) = tokio::join!(handle1, handle2);

    assert!(result1.unwrap().is_ok());
    assert!(result2.unwrap().is_ok());
    assert!(service1.was_start_called());
    assert!(service2.was_start_called());
}

#[tokio::test]
async fn service_provider_mixed_results() {
    let successful = MockSuccessfulService::new("SuccessfulService", 0, false);
    let failing = MockFailingService::new("FailingService", "Error message");
    let mut task_manager = TaskManager::new();

    let success_result = successful.start(&mut task_manager).await;
    let fail_result = failing.start(&mut task_manager).await;

    assert!(success_result.is_ok());
    assert!(fail_result.is_err());
    assert!(successful.was_start_called());

    // Cleanup
    task_manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn provider_error_propagation() {
    let failing_provider: MockFailingProvider = MockFailingProvider::new("Critical failure");
    let failing_service = MockFailingService::new("TestService", "Service error");

    let provider_result: Result<String> = failing_provider.provide().await;
    let service_result = failing_service.start(&mut TaskManager::new()).await;

    assert!(provider_result.is_err());
    assert!(service_result.is_err());

    let provider_error = provider_result.unwrap_err();
    let service_error = service_result.unwrap_err();

    assert!(provider_error.to_string().contains("Critical failure"));
    assert!(service_error.to_string().contains("Service error"));
}

#[tokio::test]
async fn service_lifecycle_simulation() {
    let service = MockSuccessfulService::new("LifecycleService", 10, true);
    let mut task_manager = TaskManager::new();

    // 1. Service starts successfully
    let start_result = service.start(&mut task_manager).await;
    assert!(start_result.is_ok());
    assert!(service.was_start_called());
    std::assert_eq!(task_manager.active_count(), 1);

    // 2. Give task time to execute
    sleep(Duration::from_millis(10)).await;
    assert!(service.was_task_spawned());

    // 3. Shutdown
    let shutdown_result = task_manager.shutdown_all().await;
    assert!(shutdown_result.is_ok());
    std::assert_eq!(task_manager.active_count(), 0);
}
