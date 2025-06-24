use super::super::fan_controller::FanController;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

/// Type alias for channel color mapping to reduce type complexity
type ChannelColorMap = HashMap<u8, (u8, u8, u8)>;

// Mock controller that succeeds all operations
#[derive(Debug)]
struct MockSuccessfulController {
    active_curves: Arc<Mutex<HashMap<u8, String>>>,
    last_temperatures: Arc<Mutex<HashMap<u8, f32>>>,
    last_speeds: Arc<Mutex<HashMap<u8, u8>>>,
    channel_colors: Arc<Mutex<ChannelColorMap>>,
    init_called: Arc<Mutex<bool>>,
    firmware: (u8, u8, u8),
}

impl MockSuccessfulController {
    fn new(_controller_id: u8) -> Self {
        Self {
            active_curves: Arc::new(Mutex::new(HashMap::new())),
            last_temperatures: Arc::new(Mutex::new(HashMap::new())),
            last_speeds: Arc::new(Mutex::new(HashMap::new())),
            channel_colors: Arc::new(Mutex::new(HashMap::new())),
            init_called: Arc::new(Mutex::new(false)),
            firmware: (1, 2, 3),
        }
    }

    fn was_init_called(&self) -> bool {
        *self.init_called.lock().unwrap()
    }

    fn get_last_temperature(&self, channel: u8) -> Option<f32> {
        self.last_temperatures
            .lock()
            .unwrap()
            .get(&channel)
            .copied()
    }

    fn get_last_speed(&self, channel: u8) -> Option<u8> {
        self.last_speeds
            .lock()
            .unwrap()
            .get(&channel)
            .copied()
    }

    fn get_channel_color(&self, channel: u8) -> Option<(u8, u8, u8)> {
        self.channel_colors.lock().unwrap().get(&channel).copied()
    }
}

#[async_trait]
impl FanController for MockSuccessfulController {
    async fn send_init(&self) -> Result<()> {
        *self.init_called.lock().unwrap() = true;
        Ok(())
    }

    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> {
        self.last_temperatures.lock().unwrap().insert(channel, temp);
        self.last_speeds.lock().unwrap().insert(channel, speed);
        Ok(())
    }

    async fn update_channel_batch(&self, batch: Vec<(usize, f32, u8)>) -> Result<()> {
        for (channel, temp, speed) in batch {
            self.update_channel(channel as u8, temp, speed).await?;
        }
        Ok(())
    }

    async fn update_channel_color(
        &self,
        channel: u8,
        red: u8,
        green: u8,
        blue: u8,
    ) -> Result<()> {
        self.channel_colors
            .lock()
            .unwrap()
            .insert(channel, (red, green, blue));
        Ok(())
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        Ok(self.firmware)
    }
}

// Mock controller that fails operations
#[derive(Debug)]
struct MockFailingController {
    error_message: String,
}

impl MockFailingController {
    fn new(error_message: &str) -> Self {
        Self {
            error_message: error_message.to_string(),
        }
    }
}

#[async_trait]
impl FanController for MockFailingController {
    async fn send_init(&self) -> Result<()> {
        Err(anyhow!("Init failed: {}", self.error_message))
    }

    async fn update_channel(&self, _channel: u8, _temp: f32, _speed: u8) -> Result<()> {
        Err(anyhow!("Update channel failed: {}", self.error_message))
    }

    async fn update_channel_batch(&self, _batch: Vec<(usize, f32, u8)>) -> Result<()> {
        Err(anyhow!("Update batch failed: {}", self.error_message))
    }

    async fn update_channel_color(
        &self,
        _channel: u8,
        _red: u8,
        _green: u8,
        _blue: u8,
    ) -> Result<()> {
        Err(anyhow!("Update color failed: {}", self.error_message))
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        Err(anyhow!("Firmware version failed: {}", self.error_message))
    }
}

// Mock controller with delay for async testing
#[derive(Debug)]
struct MockSlowController {
    delay_ms: u64,
    inner: MockSuccessfulController,
}

impl MockSlowController {
    fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            inner: MockSuccessfulController::new(0),
        }
    }
}

#[async_trait]
impl FanController for MockSlowController {
    async fn send_init(&self) -> Result<()> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner.send_init().await
    }

    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner.update_channel(channel, temp, speed).await
    }

    async fn update_channel_batch(&self, batch: Vec<(usize, f32, u8)>) -> Result<()> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner.update_channel_batch(batch).await
    }

    async fn update_channel_color(
        &self,
        channel: u8,
        red: u8,
        green: u8,
        blue: u8,
    ) -> Result<()> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner
            .update_channel_color(channel, red, green, blue)
            .await
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.inner.firmware_version().await
    }
}

#[tokio::test]
async fn successful_controller_init() {
    let controller = MockSuccessfulController::new(0);

    assert!(!controller.was_init_called());
    let result = controller.send_init().await;

    assert!(result.is_ok());
    assert!(controller.was_init_called());
}

#[tokio::test]
async fn successful_controller_update_channel() {
    let controller = MockSuccessfulController::new(0);

    let result = controller.update_channel(2, 42.0, 75).await;
    assert!(result.is_ok());
    std::assert_eq!(controller.get_last_temperature(2), Some(42.0));
    std::assert_eq!(controller.get_last_speed(2), Some(75));
    std::assert_eq!(controller.get_last_temperature(1), None); // Other channels unaffected
}

#[tokio::test]
async fn successful_controller_update_batch() {
    let controller = MockSuccessfulController::new(0);

    let batch = vec![(1, 30.0, 50), (2, 45.0, 75), (3, 60.0, 90)];
    let result = controller.update_channel_batch(batch).await;
    
    assert!(result.is_ok());
    std::assert_eq!(controller.get_last_temperature(1), Some(30.0));
    std::assert_eq!(controller.get_last_speed(1), Some(50));
    std::assert_eq!(controller.get_last_temperature(2), Some(45.0));
    std::assert_eq!(controller.get_last_speed(2), Some(75));
    std::assert_eq!(controller.get_last_temperature(3), Some(60.0));
    std::assert_eq!(controller.get_last_speed(3), Some(90));
}

#[tokio::test]
async fn successful_controller_update_color() {
    let controller = MockSuccessfulController::new(0);

    let result = controller.update_channel_color(1, 255, 128, 64).await;
    assert!(result.is_ok());
    std::assert_eq!(controller.get_channel_color(1), Some((255, 128, 64)));
}

#[tokio::test]
async fn successful_controller_firmware_version() {
    let controller = MockSuccessfulController::new(0);

    let result = controller.firmware_version().await;
    assert!(result.is_ok());
    std::assert_eq!(result.unwrap(), (1, 2, 3));
}

#[tokio::test]
async fn failing_controller_all_operations() {
    let controller = MockFailingController::new("Hardware error");

    assert!(controller.send_init().await.is_err());
    assert!(controller.update_channel(1, 50.0, 75).await.is_err());
    assert!(controller.update_channel_color(0, 255, 0, 0).await.is_err());
    assert!(controller.firmware_version().await.is_err());
    assert!(controller.update_channel_batch(vec![(1, 30.0, 50)]).await.is_err());
}

#[tokio::test]
async fn slow_controller_timing() {
    let controller = MockSlowController::new(50);

    let start = std::time::Instant::now();
    let result = controller.send_init().await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert!(duration.as_millis() >= 50);
}

#[tokio::test]
async fn controller_trait_object_compatibility() {
    let controllers: Vec<Box<dyn FanController>> = vec![
        Box::new(MockSuccessfulController::new(0)),
        Box::new(MockSuccessfulController::new(1)),
    ];

    for controller in &controllers {
        let result = controller.send_init().await;
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn concurrent_controller_operations() {
    let controller = Arc::new(MockSuccessfulController::new(0));

    let mut handles = vec![];

    // Spawn multiple concurrent operations
    for i in 0..5 {
        let controller_clone = controller.clone();
        let handle = tokio::spawn(async move {
            controller_clone
                .update_channel(i, i as f32 * 10.0, (i * 20) as u8)
                .await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    // Verify all channels were updated
    for i in 0..5 {
        std::assert_eq!(controller.get_last_temperature(i), Some(i as f32 * 10.0));
        std::assert_eq!(controller.get_last_speed(i), Some((i * 20) as u8));
    }
}

#[tokio::test]
async fn controller_rgb_color_boundaries() {
    let controller = MockSuccessfulController::new(0);

    // Test boundary RGB values
    let test_colors = [
        (0, 0, 0),       // Black
        (255, 255, 255), // White
        (255, 0, 0),     // Red
        (0, 255, 0),     // Green
        (0, 0, 255),     // Blue
    ];

    for (i, (r, g, b)) in test_colors.iter().enumerate() {
        let result = controller.update_channel_color(i as u8, *r, *g, *b).await;
        assert!(result.is_ok());
        std::assert_eq!(controller.get_channel_color(i as u8), Some((*r, *g, *b)));
    }
}

#[tokio::test]
async fn controller_channel_boundaries() {
    let controller = MockSuccessfulController::new(0);

    // Test with extreme channel values
    let result1 = controller.update_channel(0, 50.0, 25).await; // Min channel
    let result2 = controller.update_channel(255, 60.0, 100).await; // Max channel

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[tokio::test]
async fn controller_mixed_success_failure() {
    let controllers: Vec<Box<dyn FanController>> = vec![
        Box::new(MockSuccessfulController::new(0)),
        Box::new(MockFailingController::new("Error")),
        Box::new(MockSlowController::new(10)),
    ];

    let mut results = vec![];
    for controller in controllers {
        let result = controller.send_init().await;
        results.push(result);
    }

    assert!(results[0].is_ok()); // Successful
    assert!(results[1].is_err()); // Failing
    assert!(results[2].is_ok()); // Slow but successful
}

#[tokio::test]
async fn controller_debug_trait() {
    let controller = MockSuccessfulController::new(42);
    let debug_output = format!("{:?}", controller);
    assert!(debug_output.contains("MockSuccessfulController"));
}

#[tokio::test]
async fn controller_error_message_content() {
    let controller = MockFailingController::new("Specific hardware error");

    let result = controller.send_init().await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Init failed"));
    assert!(error_msg.contains("Specific hardware error"));
}

#[tokio::test]
async fn controller_temperature_speed_ranges() {
    let controller = MockSuccessfulController::new(0);

    // Test various temperature and speed ranges
    let test_cases = [
        (0.0, 0),      // Minimum values
        (25.0, 25),    // Low values
        (50.0, 50),    // Medium values
        (75.0, 75),    // High values
        (100.0, 100),  // Maximum reasonable values
    ];

    for (i, (temp, speed)) in test_cases.iter().enumerate() {
        let result = controller.update_channel(i as u8, *temp, *speed).await;
        assert!(result.is_ok());
        std::assert_eq!(controller.get_last_temperature(i as u8), Some(*temp));
        std::assert_eq!(controller.get_last_speed(i as u8), Some(*speed));
    }
}

#[tokio::test]
async fn controller_batch_operations_edge_cases() {
    let controller = MockSuccessfulController::new(0);

    // Test empty batch
    let result = controller.update_channel_batch(vec![]).await;
    assert!(result.is_ok());

    // Test single item batch
    let result = controller.update_channel_batch(vec![(1, 45.0, 80)]).await;
    assert!(result.is_ok());
    std::assert_eq!(controller.get_last_temperature(1), Some(45.0));
    std::assert_eq!(controller.get_last_speed(1), Some(80));

    // Test large batch
    let large_batch: Vec<(usize, f32, u8)> = (0..10)
        .map(|i| (i, i as f32 * 5.0, (i * 10) as u8))
        .collect();
    let result = controller.update_channel_batch(large_batch).await;
    assert!(result.is_ok());
} 