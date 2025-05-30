use anyhow::Result;
use async_trait::async_trait;

use crate::task_manager::TaskManager;

/// Base trait for providers that can create components asynchronously.
///
/// Enables dependency injection pattern with async initialization support.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::providers::traits::AsyncProvider;
/// use std::sync::Arc;
///
/// struct ConfigProvider;
///
/// #[async_trait::async_trait]
/// impl AsyncProvider<String> for ConfigProvider {
///     async fn provide(&self) -> anyhow::Result<String> {
///         Ok("config data".to_string())
///     }
/// }
/// ```
#[async_trait]
pub trait AsyncProvider<T> {
    async fn provide(&self) -> Result<T>;
}

/// Trait for services that can be started through TaskManager.
///
/// Provides service lifecycle management with prioritization and
/// criticality classification for graceful degradation.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::providers::traits::ServiceProvider;
/// use tt_riingd::task_manager::TaskManager;
/// use anyhow::Result;
///
/// struct ExampleService;
///
/// #[async_trait::async_trait]
/// impl ServiceProvider for ExampleService {
///     async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
///         task_manager.spawn_task("example".to_string(), |_token| async {
///             // Service implementation
///             Ok(())
///         }).await
///     }
///     
///     fn name(&self) -> &'static str { "ExampleService" }
///     fn priority(&self) -> i32 { 5 }
///     fn is_critical(&self) -> bool { false }
/// }
/// ```
#[async_trait]
pub trait ServiceProvider: Send + Sync {
    /// Starts the service in TaskManager.
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()>;

    /// Returns service name for logging and management.
    fn name(&self) -> &'static str;

    /// Returns startup priority (higher numbers start first).
    fn priority(&self) -> i32 {
        0
    }

    /// Indicates if service is critical for system operation.
    fn is_critical(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_manager::TaskManager;
    use anyhow::anyhow;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::time::{Duration, sleep};
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
        assert_eq!(result.unwrap(), "test_value");
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn async_provider_successful_integer() {
        let provider = MockSuccessfulProvider::new(42i32);

        let result = provider.provide().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn async_provider_successful_complex_type() {
        let config = HashMap::from([
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ]);
        let provider = MockSuccessfulProvider::new(config.clone());

        let result = provider.provide().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), config);
    }

    #[tokio::test]
    async fn async_provider_multiple_calls() {
        let provider = MockSuccessfulProvider::new("repeated_value".to_string());

        for i in 1..=5 {
            let result = provider.provide().await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "repeated_value");
            assert_eq!(provider.call_count(), i);
        }
    }

    #[tokio::test]
    async fn async_provider_failing() {
        let provider: MockFailingProvider = MockFailingProvider::new("Provider error");
        let result: Result<String> = provider.provide().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Provider error"));
    }

    #[tokio::test]
    async fn async_provider_slow_timing() {
        let provider = MockSlowProvider::new("slow_value".to_string(), 50);

        let start = std::time::Instant::now();
        let result = provider.provide().await;
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "slow_value");
        assert!(duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn async_provider_concurrent_access() {
        let provider = Arc::new(MockSuccessfulProvider::new("concurrent_value".to_string()));

        let mut handles = vec![];
        for _ in 0..10 {
            let provider_clone = provider.clone();
            let handle = tokio::spawn(async move { provider_clone.provide().await });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "concurrent_value");
        }

        assert_eq!(provider.call_count(), 10);
    }

    #[tokio::test]
    async fn async_provider_trait_object() {
        let providers: Vec<Box<dyn AsyncProvider<String>>> = vec![
            Box::new(MockSuccessfulProvider::new("value1".to_string())),
            Box::new(MockSuccessfulProvider::new("value2".to_string())),
        ];

        for (i, provider) in providers.iter().enumerate() {
            let result = provider.provide().await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), format!("value{}", i + 1));
        }
    }

    // Tests for ServiceProvider trait

    #[tokio::test]
    async fn service_provider_successful_start() {
        let mut task_manager = TaskManager::new();
        let service = MockSuccessfulService::new("test_service", 5, false);

        assert!(!service.was_start_called());

        let result = service.start(&mut task_manager).await;
        assert!(result.is_ok());
        assert!(service.was_start_called());

        // Give time for task to execute
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(service.was_task_spawned());
    }

    #[tokio::test]
    async fn service_provider_metadata() {
        let service = MockSuccessfulService::new("metadata_service", 10, true);

        assert_eq!(service.name(), "metadata_service");
        assert_eq!(service.priority(), 10);
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
                "default_service"
            }
        }

        let service = DefaultService::new();
        assert_eq!(service.priority(), 0); // Default priority
        assert!(!service.is_critical()); // Default criticality
    }

    #[tokio::test]
    async fn service_provider_failing_start() {
        let mut task_manager = TaskManager::new();
        let service = MockFailingService::new("failing_service", "Start failed");

        let result = service.start(&mut task_manager).await;
        assert!(result.is_err());
        let error_string = result.unwrap_err().to_string();
        assert!(error_string.contains("failing_service"));
        assert!(error_string.contains("Start failed"));
    }

    #[tokio::test]
    async fn service_provider_slow_start() {
        let mut task_manager = TaskManager::new();
        let service = MockSlowService::new("slow_service", 50);

        let start = std::time::Instant::now();
        let result = service.start(&mut task_manager).await;
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert!(duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn service_provider_priority_ordering() {
        let services = vec![
            MockSuccessfulService::new("low_priority", 1, false),
            MockSuccessfulService::new("high_priority", 10, true),
            MockSuccessfulService::new("medium_priority", 5, false),
        ];

        let mut sorted_services = services;
        sorted_services.sort_by_key(|b| std::cmp::Reverse(b.priority())); // Higher priority first

        assert_eq!(sorted_services[0].name(), "high_priority");
        assert_eq!(sorted_services[1].name(), "medium_priority");
        assert_eq!(sorted_services[2].name(), "low_priority");
    }

    #[tokio::test]
    async fn service_provider_criticality_classification() {
        let critical_service = MockSuccessfulService::new("critical", 5, true);
        let non_critical_service = MockSuccessfulService::new("non_critical", 5, false);

        assert!(critical_service.is_critical());
        assert!(!non_critical_service.is_critical());
    }

    #[tokio::test]
    async fn service_provider_trait_object() {
        let mut task_manager = TaskManager::new();
        let services: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(MockSuccessfulService::new("service1", 1, false)),
            Box::new(MockSuccessfulService::new("service2", 2, true)),
        ];

        for service in &services {
            let result = service.start(&mut task_manager).await;
            assert!(result.is_ok());
            assert!(service.name().starts_with("service"));
        }
    }

    #[tokio::test]
    async fn service_provider_concurrent_starts() {
        let services = vec![
            Arc::new(MockSuccessfulService::new("concurrent1", 1, false)),
            Arc::new(MockSuccessfulService::new("concurrent2", 2, false)),
            Arc::new(MockSuccessfulService::new("concurrent3", 3, false)),
        ];

        let mut handles = vec![];
        for service in services {
            let handle = tokio::spawn(async move {
                let mut task_manager = TaskManager::new();
                service.start(&mut task_manager).await
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn service_provider_mixed_results() {
        let mut task_manager = TaskManager::new();
        let services: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(MockSuccessfulService::new("success", 1, false)),
            Box::new(MockFailingService::new("failure", "Failed to start")),
            Box::new(MockSlowService::new("slow", 10)),
        ];

        let mut results = vec![];
        for service in services {
            let result = service.start(&mut task_manager).await;
            results.push(result);
        }

        assert!(results[0].is_ok()); // Successful
        assert!(results[1].is_err()); // Failing
        assert!(results[2].is_ok()); // Slow but successful
    }

    #[tokio::test]
    async fn provider_error_propagation() {
        let failing_provider: MockFailingProvider =
            MockFailingProvider::new("Custom error message");
        let result: Result<i32> = failing_provider.provide().await;

        match result {
            Err(e) => {
                assert_eq!(e.to_string(), "Custom error message");
            }
            Ok(_) => panic!("Expected error but got success"),
        }
    }

    #[tokio::test]
    async fn service_lifecycle_simulation() {
        let mut task_manager = TaskManager::new();

        // Simulate service startup sequence
        let services = vec![
            MockSuccessfulService::new("database", 10, true), // High priority, critical
            MockSuccessfulService::new("cache", 5, false),    // Medium priority, non-critical
            MockSuccessfulService::new("web_server", 1, true), // Low priority, critical
        ];

        // Sort by priority (higher first)
        let mut sorted_services = services;
        sorted_services.sort_by_key(|b| std::cmp::Reverse(b.priority()));

        // Start services in priority order
        for service in &sorted_services {
            let result = service.start(&mut task_manager).await;
            if service.is_critical() {
                assert!(
                    result.is_ok(),
                    "Critical service {} must start successfully",
                    service.name()
                );
            }
        }

        // Verify startup order
        assert_eq!(sorted_services[0].name(), "database");
        assert_eq!(sorted_services[1].name(), "cache");
        assert_eq!(sorted_services[2].name(), "web_server");
    }
}
