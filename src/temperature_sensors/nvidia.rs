//! NVIDIA GPU temperature monitoring via dynamic NVML loading.
//!
//! This module provides temperature sensor implementation for NVIDIA GPUs
//! using the NVIDIA Management Library (NVML). The library is loaded dynamically
//! to ensure the daemon works on systems without NVIDIA GPUs or drivers.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use libloading::{Library, Symbol};
use tracing::{debug, error, info, warn};

use crate::{config::SensorCfg, temperature_sensors::sensor::TemperatureSensor};

// NVML API constants (only the ones we actually use)
const NVML_SUCCESS: c_int = 0;
const NVML_DEVICE_NAME_BUFFER_SIZE: usize = 64;
const NVML_ERROR_ALREADY_INITIALIZED: c_int = 5;

// NVML temperature sensors
const NVML_TEMPERATURE_GPU: c_int = 0;

// NVML opaque handle types
#[repr(C)]
pub struct NvmlDevice {
    _unused: [u8; 0],
}

// NVML function type definitions
type NvmlInitFn = unsafe extern "C" fn() -> c_int;
type NvmlShutdownFn = unsafe extern "C" fn() -> c_int;
type NvmlDeviceGetCountFn = unsafe extern "C" fn(*mut c_uint) -> c_int;
type NvmlDeviceGetHandleByIndexFn = unsafe extern "C" fn(c_uint, *mut *mut NvmlDevice) -> c_int;
type NvmlDeviceGetNameFn = unsafe extern "C" fn(*mut NvmlDevice, *mut c_char, c_uint) -> c_int;
type NvmlDeviceGetTemperatureFn =
    unsafe extern "C" fn(*mut NvmlDevice, c_int, *mut c_uint) -> c_int;
type NvmlErrorStringFn = unsafe extern "C" fn(c_int) -> *const c_char;

/// NVML library wrapper with safe symbol management.
///
/// Uses a two-phase approach: first loads library, then extracts function pointers
/// for safe storage. This avoids self-referential struct issues while maintaining safety.
pub struct NvmlLibrary {
    _lib: Library, // ← Keeps library loaded
    // Function pointers extracted from symbols - safe as long as _lib is alive
    init: NvmlInitFn,
    shutdown: NvmlShutdownFn,
    device_get_count: NvmlDeviceGetCountFn,
    device_get_handle_by_index: NvmlDeviceGetHandleByIndexFn,
    device_get_name: NvmlDeviceGetNameFn,
    device_get_temperature: NvmlDeviceGetTemperatureFn,
    error_string: NvmlErrorStringFn,
}

impl Drop for NvmlLibrary {
    fn drop(&mut self) {
        unsafe {
            let result = (self.shutdown)();
            if result == NVML_SUCCESS {
                debug!("NVML shutdown successfully");
            } else {
                warn!("NVML shutdown failed: {}", self.error_string(result));
            }
        }
    }
}

impl NvmlLibrary {
    /// Loads the NVML library dynamically at runtime.
    pub fn load() -> Result<Self> {
        let lib_names = [
            "libnvidia-ml.so.1", // Most common
            "libnvidia-ml.so",   // Fallback
            "nvidia-ml",         // Windows-style name (if ever needed)
        ];

        let mut last_error = None;

        for lib_name in &lib_names {
            match unsafe { Library::new(lib_name) } {
                Ok(lib) => {
                    debug!("Successfully loaded NVML library: {}", lib_name);
                    return Self::from_library(lib);
                }
                Err(e) => {
                    debug!("Failed to load {}: {}", lib_name, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow!("Failed to load NVML library: No specific error available")))
    }

    /// Creates NvmlLibrary from a loaded Library instance.
    fn from_library(lib: Library) -> Result<Self> {
        unsafe {
            // Get symbols first, then extract function pointers
            let init_symbol: Symbol<NvmlInitFn> = lib
                .get(b"nvmlInit_v2\0")
                .or_else(|_| lib.get(b"nvmlInit\0"))
                .map_err(|e| anyhow!("nvmlInit function not found: {}", e))?;
            let init = *init_symbol;

            let shutdown_symbol: Symbol<NvmlShutdownFn> = lib
                .get(b"nvmlShutdown\0")
                .map_err(|e| anyhow!("nvmlShutdown function not found: {}", e))?;
            let shutdown = *shutdown_symbol;

            let device_get_count_symbol: Symbol<NvmlDeviceGetCountFn> = lib
                .get(b"nvmlDeviceGetCount_v2\0")
                .or_else(|_| lib.get(b"nvmlDeviceGetCount\0"))
                .map_err(|e| anyhow!("nvmlDeviceGetCount function not found: {}", e))?;
            let device_get_count = *device_get_count_symbol;

            let device_get_handle_by_index_symbol: Symbol<NvmlDeviceGetHandleByIndexFn> = lib
                .get(b"nvmlDeviceGetHandleByIndex_v2\0")
                .or_else(|_| lib.get(b"nvmlDeviceGetHandleByIndex\0"))
                .map_err(|e| anyhow!("nvmlDeviceGetHandleByIndex function not found: {}", e))?;
            let device_get_handle_by_index = *device_get_handle_by_index_symbol;

            let device_get_name_symbol: Symbol<NvmlDeviceGetNameFn> = lib
                .get(b"nvmlDeviceGetName\0")
                .map_err(|e| anyhow!("nvmlDeviceGetName function not found: {}", e))?;
            let device_get_name = *device_get_name_symbol;

            let device_get_temperature_symbol: Symbol<NvmlDeviceGetTemperatureFn> = lib
                .get(b"nvmlDeviceGetTemperature\0")
                .map_err(|e| anyhow!("nvmlDeviceGetTemperature function not found: {}", e))?;
            let device_get_temperature = *device_get_temperature_symbol;

            let error_string_symbol: Symbol<NvmlErrorStringFn> = lib
                .get(b"nvmlErrorString\0")
                .map_err(|e| anyhow!("nvmlErrorString function not found: {}", e))?;
            let error_string = *error_string_symbol;

            Ok(Self {
                _lib: lib,
                init,
                shutdown,
                device_get_count,
                device_get_handle_by_index,
                device_get_name,
                device_get_temperature,
                error_string,
            })
        }
    }

    /// Gets a human-readable error string for an NVML return code.
    fn error_string(&self, result: c_int) -> String {
        unsafe {
            let ptr = (self.error_string)(result);
            if ptr.is_null() {
                format!("Unknown NVML error: {}", result)
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    /// Initializes NVML - must be called before using NVML functions.
    pub fn init(&self) -> Result<()> {
        unsafe {
            let result = (self.init)();
            if result == NVML_SUCCESS || result == NVML_ERROR_ALREADY_INITIALIZED {
                debug!("NVML initialized successfully");
                Ok(())
            } else {
                Err(anyhow!(
                    "NVML initialization failed: {}",
                    self.error_string(result)
                ))
            }
        }
    }

    /// Gets the number of NVIDIA devices.
    fn device_count(&self) -> Result<u32> {
        unsafe {
            let mut count: c_uint = 0;
            let result = (self.device_get_count)(&mut count);
            if result == NVML_SUCCESS {
                Ok(count)
            } else {
                Err(anyhow!(
                    "Failed to get device count: {}",
                    self.error_string(result)
                ))
            }
        }
    }

    /// Gets a device handle by index.
    fn device_handle(&self, index: u32) -> Result<*mut NvmlDevice> {
        unsafe {
            let mut device: *mut NvmlDevice = std::ptr::null_mut();
            let result = (self.device_get_handle_by_index)(index, &mut device);
            if result == NVML_SUCCESS {
                Ok(device)
            } else {
                Err(anyhow!(
                    "Failed to get device handle {}: {}",
                    index,
                    self.error_string(result)
                ))
            }
        }
    }

    /// Gets the name of a device.
    fn device_name(&self, device: *mut NvmlDevice) -> Result<String> {
        unsafe {
            let mut name_buffer = [0i8; NVML_DEVICE_NAME_BUFFER_SIZE];
            let result = (self.device_get_name)(
                device,
                name_buffer.as_mut_ptr(),
                NVML_DEVICE_NAME_BUFFER_SIZE as c_uint,
            );
            if result == NVML_SUCCESS {
                let cstr = CStr::from_ptr(name_buffer.as_ptr());
                Ok(cstr.to_string_lossy().into_owned())
            } else {
                Err(anyhow!(
                    "Failed to get device name: {}",
                    self.error_string(result)
                ))
            }
        }
    }

    /// Gets the temperature of a device.
    fn device_temperature(&self, device: *mut NvmlDevice) -> Result<u32> {
        unsafe {
            let mut temp: c_uint = 0;
            let result = (self.device_get_temperature)(device, NVML_TEMPERATURE_GPU, &mut temp);
            if result == NVML_SUCCESS {
                Ok(temp)
            } else {
                Err(anyhow!(
                    "Failed to get device temperature: {}",
                    self.error_string(result)
                ))
            }
        }
    }
}

/// GPU information for tracking devices.
#[derive(Debug, Clone)]
struct GpuInfo {
    index: u32,
    name: String,
}

/// Global NVML instance.
///
/// Uses LazyLock for thread-safe lazy initialization - NVML is loaded only when first needed.
/// This is appropriate for NVML because:
/// 1. NVML is a singleton by design (one instance per process)
/// 2. Lazy loading avoids overhead on systems without NVIDIA GPUs
/// 3. The library needs to stay loaded for the entire application lifetime
/// 4. Thread-safe access is required for multiple sensor instances
///
/// Alternative architectures (DI via AppState) could be considered for better testability,
/// but the current approach follows established NVIDIA ecosystem patterns.
///
/// Returns None if NVML is not available on the system.
static NVML: LazyLock<Option<Arc<Mutex<NvmlLibrary>>>> =
    LazyLock::new(|| match NvmlLibrary::load() {
        Ok(nvml) => {
            if let Err(e) = nvml.init() {
                warn!("NVML library loaded but initialization failed: {}", e);
                return None;
            }
            info!("NVML initialized successfully");
            Some(Arc::new(Mutex::new(nvml)))
        }
        Err(e) => {
            info!("NVML not available: {}. NVIDIA GPU monitoring disabled.", e);
            None
        }
    });

/// NVIDIA GPU temperature sensor.
///
/// Provides access to NVIDIA GPU temperature through the NVML library
/// with proper async handling of blocking operations.
pub struct NvidiaSensor {
    gpu_info: GpuInfo,
    sensor_key: String,
}

impl NvidiaSensor {
    /// Discovers available NVIDIA GPUs from configuration.
    ///
    /// Scans available GPUs and creates sensor instances for each
    /// configured GPU or all available GPUs if no specific configuration.
    pub fn discover(cfg: &[SensorCfg]) -> Vec<Box<dyn TemperatureSensor>> {
        let nvml = match NVML.as_ref() {
            Some(nvml) => nvml,
            None => {
                debug!("NVML not available, skipping NVIDIA sensor discovery");
                return Vec::new();
            }
        };

        let nvml = match nvml.lock() {
            Ok(nvml) => nvml,
            Err(e) => {
                error!("Failed to lock NVML mutex: {}", e);
                return Vec::new();
            }
        };

        let device_count = match nvml.device_count() {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to get NVIDIA device count: {}", e);
                return Vec::new();
            }
        };

        debug!("Found {} NVIDIA GPU(s)", device_count);

        let mut sensors = Vec::new();

        // Get NVIDIA sensor configurations
        let nvidia_configs: Vec<_> = cfg
            .iter()
            .filter_map(|c| match c {
                SensorCfg::Nvidia { id, gpu_index } => Some((id, gpu_index)),
                _ => None,
            })
            .collect();

        if nvidia_configs.is_empty() {
            // No specific configuration, discover all GPUs
            for i in 0..device_count {
                if let Ok(sensor) =
                    Self::create_sensor_for_gpu(&nvml, i, &format!("nvidia_gpu_{}", i))
                {
                    sensors.push(sensor);
                }
            }
        } else {
            // Use specific configuration
            for (id, gpu_index) in nvidia_configs {
                if *gpu_index < device_count {
                    if let Ok(sensor) = Self::create_sensor_for_gpu(&nvml, *gpu_index, id) {
                        sensors.push(sensor);
                    }
                } else {
                    warn!(
                        "GPU index {} not found (only {} GPUs available)",
                        gpu_index, device_count
                    );
                }
            }
        }

        info!("Initialized {} NVIDIA temperature sensors", sensors.len());
        sensors
    }

    /// Creates a sensor for a specific GPU.
    fn create_sensor_for_gpu(
        nvml: &NvmlLibrary,
        gpu_index: u32,
        sensor_id: &str,
    ) -> Result<Box<dyn TemperatureSensor>> {
        let handle = nvml.device_handle(gpu_index)?;
        let name = nvml.device_name(handle)?;

        let gpu_info = GpuInfo {
            index: gpu_index,
            name: name.clone(),
        };

        debug!("Created NVIDIA sensor for GPU {}: {}", gpu_index, name);
        info!(
            "NVIDIA GPU sensor '{}' ready: {} (GPU {})",
            sensor_id, name, gpu_index
        );

        Ok(Box::new(Self {
            gpu_info,
            sensor_key: sensor_id.to_string(),
        }))
    }
}

#[async_trait]
impl TemperatureSensor for NvidiaSensor {
    async fn read_temperature(&self) -> Result<f32> {
        let nvml = NVML.as_ref().ok_or_else(|| anyhow!("NVML not available"))?;

        let gpu_index = self.gpu_info.index;
        let gpu_name = self.gpu_info.name.clone();
        let nvml_arc = nvml.clone();

        tokio::task::spawn_blocking(move || {
            let nvml = nvml_arc
                .lock()
                .map_err(|e| anyhow!("Failed to lock NVML mutex for GPU '{}': {}", gpu_name, e))?;

            // Get fresh handle each time to avoid Send issues
            let handle = nvml.device_handle(gpu_index).map_err(|e| {
                anyhow!(
                    "Failed to get handle for GPU '{}' (index {}): {}",
                    gpu_name,
                    gpu_index,
                    e
                )
            })?;
            let temp = nvml.device_temperature(handle).map_err(|e| {
                anyhow!("Failed to read temperature from GPU '{}': {}", gpu_name, e)
            })?;
            Ok(temp as f32)
        })
        .await
        .map_err(|e| anyhow!("Blocking task failed: {}", e))?
    }

    fn key(&self) -> String {
        self.sensor_key.clone()
    }
}

#[cfg(test)]
#[path = "tests/nvidia_test.rs"]
mod nvidia_test;
