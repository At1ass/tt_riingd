//! Fan controller abstraction and trait definitions.

use anyhow::Result;
use async_trait::async_trait;

/// Trait for fan controller hardware implementations.
///
/// Provides a unified interface for controlling fan speed, RGB lighting,
/// and curve management across different hardware types.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::drivers::fan_controller::FanController;
/// use anyhow::Result;
///
/// struct MockController;
///
/// #[async_trait::async_trait]
/// impl FanController for MockController {
///     async fn send_init(&self) -> Result<()> { Ok(()) }
///     async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> { Ok(()) }
///     async fn update_channel_color(&self, channel: u8, r: u8, g: u8, b: u8) -> Result<()> { Ok(()) }
///     async fn firmware_version(&self) -> Result<(u8, u8, u8)> { Ok((1, 0, 0)) }
/// }
/// impl std::fmt::Debug for MockController {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "MockController") }
/// }
/// ```
#[async_trait]
pub trait FanController: Send + Sync + core::fmt::Debug {
    /// Initializes the controller hardware.
    async fn send_init(&self) -> Result<()>;

    /// Updates fan speed for a specific channel based on temperature.
    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()>;

    /// Sets RGB color for a specific channel.
    async fn update_channel_color(&self, _channel: u8, red: u8, green: u8, blue: u8) -> Result<()>;

    /// Returns the firmware version as (major, minor, patch).
    async fn firmware_version(&self) -> Result<(u8, u8, u8)>;
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use anyhow::anyhow;
//     use std::collections::HashMap;
//     use std::sync::{Arc, Mutex};
//
//     /// Type alias for channel color mapping to reduce type complexity
//     type ChannelColorMap = HashMap<u8, (u8, u8, u8)>;
//     use tokio::time::{Duration, sleep};
//
//     // Mock controller that succeeds all operations
//     #[derive(Debug)]
//     struct MockSuccessfulController {
//         active_curves: Arc<Mutex<HashMap<u8, String>>>,
//         last_temperatures: Arc<Mutex<HashMap<u8, f32>>>,
//         channel_colors: Arc<Mutex<ChannelColorMap>>,
//         init_called: Arc<Mutex<bool>>,
//         firmware: (u8, u8, u8),
//     }
//
//     impl MockSuccessfulController {
//         fn new(_controller_id: u8) -> Self {
//             Self {
//                 active_curves: Arc::new(Mutex::new(HashMap::new())),
//                 last_temperatures: Arc::new(Mutex::new(HashMap::new())),
//                 channel_colors: Arc::new(Mutex::new(HashMap::new())),
//                 init_called: Arc::new(Mutex::new(false)),
//                 firmware: (1, 2, 3),
//             }
//         }
//
//         fn was_init_called(&self) -> bool {
//             *self.init_called.lock().unwrap()
//         }
//
//         fn get_last_temperature(&self, channel: u8) -> Option<f32> {
//             self.last_temperatures
//                 .lock()
//                 .unwrap()
//                 .get(&channel)
//                 .copied()
//         }
//
//         fn get_channel_color(&self, channel: u8) -> Option<(u8, u8, u8)> {
//             self.channel_colors.lock().unwrap().get(&channel).copied()
//         }
//     }
//
//     #[async_trait]
//     impl FanController for MockSuccessfulController {
//         async fn send_init(&self) -> Result<()> {
//             *self.init_called.lock().unwrap() = true;
//             Ok(())
//         }
//
//         async fn update_channel(&self, channel: u8, temp: f32) -> Result<()> {
//             self.last_temperatures.lock().unwrap().insert(channel, temp);
//             Ok(())
//         }
//
//         async fn update_channel_color(
//             &self,
//             channel: u8,
//             red: u8,
//             green: u8,
//             blue: u8,
//         ) -> Result<()> {
//             self.channel_colors
//                 .lock()
//                 .unwrap()
//                 .insert(channel, (red, green, blue));
//             Ok(())
//         }
//
//
//         async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
//             Ok(self.firmware)
//         }
//     }
//
//     // Mock controller that fails operations
//     #[derive(Debug)]
//     struct MockFailingController {
//         error_message: String,
//     }
//
//     impl MockFailingController {
//         fn new(error_message: &str) -> Self {
//             Self {
//                 error_message: error_message.to_string(),
//             }
//         }
//     }
//
//     #[async_trait]
//     impl FanController for MockFailingController {
//         async fn send_init(&self) -> Result<()> {
//             Err(anyhow!("Init failed: {}", self.error_message))
//         }
//
//         async fn update_channel(&self, _idx: u8, _temp: f32) -> Result<()> {
//             Err(anyhow!("Update speeds failed: {}", self.error_message))
//         }
//
//         async fn update_channel_color(
//             &self,
//             _channel: u8,
//             _red: u8,
//             _green: u8,
//             _blue: u8,
//         ) -> Result<()> {
//             Err(anyhow!("Update color failed: {}", self.error_message))
//         }
//
//         async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
//             Err(anyhow!("Firmware version failed: {}", self.error_message))
//         }
//     }
//
//     // Mock controller with delay for async testing
//     #[derive(Debug)]
//     struct MockSlowController {
//         delay_ms: u64,
//         inner: MockSuccessfulController,
//     }
//
//     impl MockSlowController {
//         fn new(delay_ms: u64) -> Self {
//             Self {
//                 delay_ms,
//                 inner: MockSuccessfulController::new(0),
//             }
//         }
//     }
//
//     #[async_trait]
//     impl FanController for MockSlowController {
//         async fn send_init(&self) -> Result<()> {
//             sleep(Duration::from_millis(self.delay_ms)).await;
//             self.inner.send_init().await
//         }
//
//         async fn update_channel(&self, idx: u8, temp: f32) -> Result<()> {
//             sleep(Duration::from_millis(self.delay_ms)).await;
//             self.inner.update_channel(idx, temp).await
//         }
//
//         async fn update_channel_color(
//             &self,
//             channel: u8,
//             red: u8,
//             green: u8,
//             blue: u8,
//         ) -> Result<()> {
//             sleep(Duration::from_millis(self.delay_ms)).await;
//             self.inner
//                 .update_channel_color(channel, red, green, blue)
//                 .await
//         }
//
//         async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
//             sleep(Duration::from_millis(self.delay_ms)).await;
//             self.inner.firmware_version().await
//         }
//     }
//
//     #[tokio::test]
//     async fn successful_controller_init() {
//         let controller = MockSuccessfulController::new(0);
//
//         assert!(!controller.was_init_called());
//         let result = controller.send_init().await;
//
//         assert!(result.is_ok());
//         assert!(controller.was_init_called());
//     }
//
//     #[tokio::test]
//     async fn successful_controller_update_channel() {
//         let controller = MockSuccessfulController::new(0);
//
//         let result = controller.update_channel(2, 42.0).await;
//         assert!(result.is_ok());
//         assert_eq!(controller.get_last_temperature(2), Some(42.0));
//         assert_eq!(controller.get_last_temperature(1), None); // Other channels unaffected
//     }
//
//     #[tokio::test]
//     async fn successful_controller_update_color() {
//         let controller = MockSuccessfulController::new(0);
//
//         let result = controller.update_channel_color(1, 255, 128, 64).await;
//         assert!(result.is_ok());
//         assert_eq!(controller.get_channel_color(1), Some((255, 128, 64)));
//     }
//
//     #[tokio::test]
//     async fn successful_controller_firmware_version() {
//         let controller = MockSuccessfulController::new(0);
//
//         let result = controller.firmware_version().await;
//         assert!(result.is_ok());
//         assert_eq!(result.unwrap(), (1, 2, 3));
//     }
//
//     #[tokio::test]
//     async fn failing_controller_all_operations() {
//         let controller = MockFailingController::new("Hardware error");
//
//         assert!(controller.send_init().await.is_err());
//         assert!(controller.update_channel_color(0, 255, 0, 0).await.is_err());
//         assert!(controller.firmware_version().await.is_err());
//     }
//
//     #[tokio::test]
//     async fn slow_controller_timing() {
//         let controller = MockSlowController::new(50);
//
//         let start = std::time::Instant::now();
//         let result = controller.send_init().await;
//         let duration = start.elapsed();
//
//         assert!(result.is_ok());
//         assert!(duration.as_millis() >= 50);
//     }
//
//     #[tokio::test]
//     async fn controller_trait_object_compatibility() {
//         let controllers: Vec<Box<dyn FanController>> = vec![
//             Box::new(MockSuccessfulController::new(0)),
//             Box::new(MockSuccessfulController::new(1)),
//         ];
//
//         for controller in &controllers {
//             let result = controller.send_init().await;
//             assert!(result.is_ok());
//         }
//     }
//
//     #[tokio::test]
//     async fn concurrent_controller_operations() {
//         let controller = Arc::new(MockSuccessfulController::new(0));
//
//         let mut handles = vec![];
//
//         // Spawn multiple concurrent operations
//         for i in 0..5 {
//             let controller_clone = controller.clone();
//             let handle =
//                 tokio::spawn(
//                     async move { controller_clone.update_channel(i, i as f32 * 10.0).await },
//                 );
//             handles.push(handle);
//         }
//
//         // Wait for all operations to complete
//         for handle in handles {
//             let result = handle.await.unwrap();
//             assert!(result.is_ok());
//         }
//
//         // Verify all channels were updated
//         for i in 0..5 {
//             assert_eq!(controller.get_last_temperature(i), Some(i as f32 * 10.0));
//         }
//     }
//
//     #[tokio::test]
//     async fn controller_rgb_color_boundaries() {
//         let controller = MockSuccessfulController::new(0);
//
//         // Test boundary RGB values
//         let test_colors = [
//             (0, 0, 0),       // Black
//             (255, 255, 255), // White
//             (255, 0, 0),     // Red
//             (0, 255, 0),     // Green
//             (0, 0, 255),     // Blue
//         ];
//
//         for (i, (r, g, b)) in test_colors.iter().enumerate() {
//             let result = controller.update_channel_color(i as u8, *r, *g, *b).await;
//             assert!(result.is_ok());
//             assert_eq!(controller.get_channel_color(i as u8), Some((*r, *g, *b)));
//         }
//     }
//
//     #[tokio::test]
//     async fn controller_channel_boundaries() {
//         let controller = MockSuccessfulController::new(0);
//
//         // Test with extreme channel values
//         let result1 = controller.update_channel(0, 50.0).await; // Min channel
//         let result2 = controller.update_channel(255, 60.0).await; // Max channel
//
//         assert!(result1.is_ok());
//         assert!(result2.is_ok());
//     }
//
//     #[tokio::test]
//     async fn controller_mixed_success_failure() {
//         let controllers: Vec<Box<dyn FanController>> = vec![
//             Box::new(MockSuccessfulController::new(0)),
//             Box::new(MockFailingController::new("Error")),
//             Box::new(MockSlowController::new(10)),
//         ];
//
//         let mut results = vec![];
//         for controller in controllers {
//             let result = controller.send_init().await;
//             results.push(result);
//         }
//
//         assert!(results[0].is_ok()); // Successful
//         assert!(results[1].is_err()); // Failing
//         assert!(results[2].is_ok()); // Slow but successful
//     }
//
//     #[tokio::test]
//     async fn controller_debug_trait() {
//         let controller = MockSuccessfulController::new(42);
//         let debug_output = format!("{:?}", controller);
//         assert!(debug_output.contains("MockSuccessfulController"));
//     }
//
//     #[tokio::test]
//     async fn controller_error_message_content() {
//         let controller = MockFailingController::new("Specific hardware error");
//
//         let result = controller.send_init().await;
//         assert!(result.is_err());
//
//         let error_msg = result.unwrap_err().to_string();
//         assert!(error_msg.contains("Init failed"));
//         assert!(error_msg.contains("Specific hardware error"));
//     }
// }
