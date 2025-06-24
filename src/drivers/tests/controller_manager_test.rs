use super::super::controller_manager::ControllerManager;
use anyhow::Result;

#[test]
fn controller_manager_empty_creates_successfully() {
    let _manager = ControllerManager::empty();
    // Test that empty controller manager can be created
}

#[tokio::test]
async fn empty_controller_manager_operations() -> Result<()> {
    let manager = ControllerManager::empty();
    
    // Send init to empty manager should succeed
    let result = manager.send_init().await;
    assert!(result.is_ok());
    
    // Trying to update channel on non-existent controller should fail
    let result = manager.update_channel(1, 1, 45.0, 50).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Device `1` not found"));
    
    Ok(())
} 