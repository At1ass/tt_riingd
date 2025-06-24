use super::super::nvidia::{NvidiaSensor, NvmlLibrary};

#[test]
fn test_nvml_library_loading() {
    // This test will only pass on systems with NVIDIA drivers
    // On other systems, it should gracefully fail
    match NvmlLibrary::load() {
        Ok(nvml) => {
            println!("NVML library loaded successfully");
            match nvml.init() {
                Ok(()) => println!("NVML initialized successfully"),
                Err(e) => println!("NVML initialization failed: {}", e),
            }
        }
        Err(e) => {
            println!("NVML library not available: {}", e);
            // This is expected on systems without NVIDIA GPUs
        }
    }
}

#[test]
fn test_nvidia_sensor_discovery() {
    let config = vec![]; // Empty config for auto-discovery
    let sensors = NvidiaSensor::discover(&config);

    // On systems with NVIDIA GPUs, this should return sensors
    // On systems without, this should return an empty vector
    println!("Discovered {} NVIDIA sensors", sensors.len());

    for sensor in &sensors {
        println!("Sensor key: {}", sensor.key());
    }
}

#[tokio::test]
async fn test_nvidia_sensor_reading() {
    let config = vec![];
    let sensors = NvidiaSensor::discover(&config);

    for sensor in sensors {
        match sensor.read_temperature().await {
            Ok(temp) => {
                println!("GPU {} temperature: {}°C", sensor.key(), temp);
                assert!(temp > 0.0 && temp < 150.0); // Reasonable temperature range
            }
            Err(e) => {
                println!("Failed to read temperature from {}: {}", sensor.key(), e);
            }
        }
    }
}
