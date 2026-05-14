//! GPU device detection and management

use crate::{GpuError, Result};
use cudarc::driver::CudaDevice;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::Nvml;
use std::any::Any;
use std::ffi::OsStr;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

/// Information about a GPU device
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Device ID
    pub device_id: usize,

    /// Device name
    pub name: String,

    /// Total memory in bytes
    pub total_memory: u64,

    /// PCI bus ID
    pub pci_bus_id: String,
}

/// Information about a CUDA device when NVML telemetry is unavailable
#[derive(Debug, Clone)]
pub struct CudaDeviceInfo {
    /// Device ID
    pub device_id: usize,

    /// Device name
    pub name: String,
}

/// GPU manager for device detection and monitoring
pub struct GpuManager {
    nvml: Nvml,
}

impl GpuManager {
    /// Initialize GPU manager and detect all NVIDIA GPUs
    pub fn new() -> Result<Self> {
        let nvml = init_nvml()?;

        let device_count = nvml
            .device_count()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        if device_count == 0 {
            return Err(GpuError::NoGpusFound);
        }

        Ok(Self { nvml })
    }

    /// Get information about all available GPUs
    pub fn list_gpus(&self) -> Result<Vec<GpuInfo>> {
        let device_count = self
            .nvml
            .device_count()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        let mut gpu_infos = Vec::new();

        for idx in 0..device_count {
            let device = self
                .nvml
                .device_by_index(idx)
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;

            let name = device
                .name()
                .unwrap_or_else(|_| format!("NVIDIA GPU {}", idx));
            let total_memory = device
                .memory_info()
                .map(|memory_info| memory_info.total)
                .unwrap_or(0);
            let pci_bus_id = device
                .pci_info()
                .map(|pci_info| {
                    format!(
                        "{:04x}:{:02x}:{:02x}.0",
                        pci_info.domain, pci_info.bus, pci_info.device
                    )
                })
                .unwrap_or_else(|_| "N/A (integrated GPU or unsupported by NVML)".to_string());

            gpu_infos.push(GpuInfo {
                device_id: idx as usize,
                name,
                total_memory,
                pci_bus_id,
            });
        }

        Ok(gpu_infos)
    }

    /// Get GPU temperature in Celsius
    pub fn get_temperature(&self, device_id: usize) -> Result<u32> {
        let device = self
            .nvml
            .device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }

    /// Get GPU power usage in milliwatts
    pub fn get_power_usage(&self, device_id: usize) -> Result<u32> {
        let device = self
            .nvml
            .device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        device
            .power_usage()
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }

    /// Get GPU utilization percentage
    pub fn get_utilization(&self, device_id: usize) -> Result<u32> {
        let device = self
            .nvml
            .device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        let utilization = device
            .utilization_rates()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;

        Ok(utilization.gpu)
    }
}

/// Enumerate CUDA devices without relying on NVML telemetry
pub fn list_cuda_devices() -> Result<Vec<CudaDeviceInfo>> {
    let device_count = match catch_unwind(AssertUnwindSafe(CudaDevice::count)) {
        Ok(Ok(count)) => count,
        Ok(Err(error)) => return Err(GpuError::CudaError(error.to_string())),
        Err(panic) => {
            return Err(GpuError::CudaError(format!(
                "CUDA driver probe panicked: {}",
                panic_message(panic)
            )))
        }
    };

    if device_count <= 0 {
        return Err(GpuError::NoGpusFound);
    }

    let mut devices = Vec::new();
    for idx in 0..device_count {
        let device =
            CudaDevice::new(idx as usize).map_err(|e| GpuError::CudaError(e.to_string()))?;
        let name = device
            .name()
            .unwrap_or_else(|_| format!("CUDA Device {}", idx));
        devices.push(CudaDeviceInfo {
            device_id: idx as usize,
            name,
        });
    }

    Ok(devices)
}

fn panic_message(panic: Box<dyn Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn init_nvml() -> Result<Nvml> {
    let mut attempts = Vec::new();

    if let Some(path) = std::env::var_os("BMINER_NVML_LIB_PATH") {
        match try_nvml_path(path.as_os_str()) {
            Ok(nvml) => return Ok(nvml),
            Err(error) => attempts.push(format!("{} ({error})", path.to_string_lossy())),
        }
    }

    for candidate in nvml_candidates() {
        match try_nvml_path(candidate.as_ref()) {
            Ok(nvml) => return Ok(nvml),
            Err(error) => {
                if !matches!(
                    error,
                    NvmlError::LibloadingError(_) | NvmlError::LibraryNotFound
                ) {
                    return Err(GpuError::NvmlError(error.to_string()));
                }
                attempts.push(format!("{} ({error})", candidate.display()));
            }
        }
    }

    Err(GpuError::NvmlError(format!(
        "unable to locate a usable NVML library. Set BMINER_NVML_LIB_PATH to your libnvidia-ml.so path. Attempts: {}",
        attempts.join(", ")
    )))
}

fn try_nvml_path(path: &OsStr) -> std::result::Result<Nvml, NvmlError> {
    let mut builder = Nvml::builder();
    builder.lib_path(path);
    builder.init()
}

fn nvml_candidates() -> Vec<&'static Path> {
    vec![
        Path::new("libnvidia-ml.so"),
        Path::new("libnvidia-ml.so.1"),
        Path::new("/usr/lib/wsl/lib/libnvidia-ml.so"),
        Path::new("/usr/lib/wsl/lib/libnvidia-ml.so.1"),
        Path::new("/usr/lib/x86_64-linux-gnu/libnvidia-ml.so"),
        Path::new("/usr/lib/x86_64-linux-gnu/libnvidia-ml.so.1"),
        Path::new("/usr/lib/aarch64-linux-gnu/libnvidia-ml.so"),
        Path::new("/usr/lib/aarch64-linux-gnu/libnvidia-ml.so.1"),
        Path::new("/usr/lib/aarch64-linux-gnu/nvidia/libnvidia-ml.so"),
        Path::new("/usr/lib/aarch64-linux-gnu/nvidia/libnvidia-ml.so.1"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires NVIDIA GPU
    fn test_gpu_detection() {
        let manager = GpuManager::new();

        // This test will fail if no GPU is present
        if let Ok(manager) = manager {
            let gpus = manager.list_gpus().unwrap();
            assert!(!gpus.is_empty());

            for gpu in gpus {
                println!("Found GPU: {} ({})", gpu.name, gpu.pci_bus_id);
            }
        }
    }
}

// Made with Bob
