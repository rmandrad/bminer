//! GPU device detection and management

use crate::{GpuError, Result};
use nvml_wrapper::Nvml;

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

/// GPU manager for device detection and monitoring
pub struct GpuManager {
    nvml: Nvml,
}

impl GpuManager {
    /// Initialize GPU manager and detect all NVIDIA GPUs
    pub fn new() -> Result<Self> {
        let nvml = Nvml::init()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        let device_count = nvml.device_count()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        if device_count == 0 {
            return Err(GpuError::NoGpusFound);
        }
        
        Ok(Self { nvml })
    }
    
    /// Get information about all available GPUs
    pub fn list_gpus(&self) -> Result<Vec<GpuInfo>> {
        let device_count = self.nvml.device_count()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        let mut gpu_infos = Vec::new();
        
        for idx in 0..device_count {
            let device = self.nvml.device_by_index(idx)
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let name = device.name()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let memory_info = device.memory_info()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let pci_info = device.pci_info()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            gpu_infos.push(GpuInfo {
                device_id: idx as usize,
                name,
                total_memory: memory_info.total,
                pci_bus_id: format!("{:04x}:{:02x}:{:02x}.0", 
                    pci_info.domain, pci_info.bus, pci_info.device),
            });
        }
        
        Ok(gpu_infos)
    }
    
    /// Get GPU temperature in Celsius
    pub fn get_temperature(&self, device_id: usize) -> Result<u32> {
        let device = self.nvml.device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }
    
    /// Get GPU power usage in milliwatts
    pub fn get_power_usage(&self, device_id: usize) -> Result<u32> {
        let device = self.nvml.device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        device.power_usage()
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }
    
    /// Get GPU utilization percentage
    pub fn get_utilization(&self, device_id: usize) -> Result<u32> {
        let device = self.nvml.device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        let utilization = device.utilization_rates()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        Ok(utilization.gpu)
    }
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
