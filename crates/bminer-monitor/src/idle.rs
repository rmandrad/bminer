//! System idle detection
//!
//! This module provides intelligent idle detection to ensure mining only
//! occurs when the system is truly idle.

use crate::{Result, SystemMetrics};
use nvml_wrapper::Nvml;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use std::time::{Duration, Instant};

/// Idle detection configuration
#[derive(Debug, Clone)]
pub struct IdleConfig {
    /// CPU usage threshold (percentage, 0-100)
    pub cpu_threshold: f32,
    
    /// Minimum idle duration before starting mining (seconds)
    pub min_idle_duration: u64,
    
    /// Check interval (seconds)
    pub check_interval: u64,
    
    /// GPU utilization threshold (percentage, 0-100)
    pub gpu_threshold: u32,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            cpu_threshold: 20.0,
            min_idle_duration: 60,
            check_interval: 5,
            gpu_threshold: 10,
        }
    }
}

/// Idle detector for system monitoring
pub struct IdleDetector {
    config: IdleConfig,
    system: System,
    nvml: Option<Nvml>,
    idle_since: Option<Instant>,
    last_check: Instant,
}

impl IdleDetector {
    /// Create a new idle detector
    pub fn new(config: IdleConfig) -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        let nvml = Nvml::init().ok();
        
        // Initial refresh
        system.refresh_cpu_all();
        
        Self {
            config,
            system,
            nvml,
            idle_since: None,
            last_check: Instant::now(),
        }
    }
    
    /// Check if system is currently idle
    pub fn is_idle(&mut self) -> Result<bool> {
        // Only check at configured intervals
        let now = Instant::now();
        if now.duration_since(self.last_check) < Duration::from_secs(self.config.check_interval) {
            return Ok(self.idle_since.is_some() && 
                     now.duration_since(self.idle_since.unwrap()) >= 
                     Duration::from_secs(self.config.min_idle_duration));
        }
        
        self.last_check = now;
        
        // Refresh system information
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        
        // Check CPU usage (average across all CPUs)
        let cpu_usage = self.system.cpus().iter()
            .map(|cpu| cpu.cpu_usage())
            .sum::<f32>() / self.system.cpus().len() as f32;
        
        tracing::debug!("CPU usage: {:.2}%", cpu_usage);
        
        if cpu_usage > self.config.cpu_threshold {
            // System is busy
            self.idle_since = None;
            return Ok(false);
        }

        if let Some(gpu_usage) = self.max_gpu_utilization()? {
            tracing::debug!("GPU usage: {}%", gpu_usage);

            if gpu_usage > self.config.gpu_threshold {
                self.idle_since = None;
                return Ok(false);
            }
        }
        
        // System appears idle
        if self.idle_since.is_none() {
            self.idle_since = Some(now);
            tracing::info!("System idle detected, waiting {} seconds before mining", 
                          self.config.min_idle_duration);
        }
        
        // Check if we've been idle long enough
        let idle_duration = now.duration_since(self.idle_since.unwrap());
        let is_idle = idle_duration >= Duration::from_secs(self.config.min_idle_duration);
        
        if is_idle {
            tracing::debug!("System has been idle for {} seconds", idle_duration.as_secs());
        }
        
        Ok(is_idle)
    }
    
    /// Get current system metrics
    pub fn get_metrics(&mut self) -> Result<SystemMetrics> {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        
        let cpu_usage = self.system.cpus().iter()
            .map(|cpu| cpu.cpu_usage())
            .sum::<f32>() / self.system.cpus().len() as f32;
        
        Ok(SystemMetrics {
            cpu_usage,
            memory_usage: self.system.used_memory(),
            timestamp: Instant::now(),
        })
    }
    
    /// Reset idle state (call when mining stops)
    pub fn reset(&mut self) {
        self.idle_since = None;
        tracing::debug!("Idle detector reset");
    }
    
    /// Get idle duration in seconds (0 if not idle)
    pub fn idle_duration(&self) -> u64 {
        if let Some(since) = self.idle_since {
            Instant::now().duration_since(since).as_secs()
        } else {
            0
        }
    }

    fn max_gpu_utilization(&self) -> Result<Option<u32>> {
        let Some(nvml) = &self.nvml else {
            return Ok(None);
        };

        let device_count = nvml
            .device_count()
            .map_err(|e| crate::MonitorError::GpuError(e.to_string()))?;
        let mut max_utilization: Option<u32> = None;

        for idx in 0..device_count {
            let device = nvml
                .device_by_index(idx)
                .map_err(|e| crate::MonitorError::GpuError(e.to_string()))?;
            let utilization = device
                .utilization_rates()
                .map_err(|e| crate::MonitorError::GpuError(e.to_string()))?;

            max_utilization = Some(match max_utilization {
                Some(current) => current.max(utilization.gpu),
                None => utilization.gpu,
            });
        }

        Ok(max_utilization)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_idle_config_default() {
        let config = IdleConfig::default();
        assert_eq!(config.cpu_threshold, 20.0);
        assert_eq!(config.min_idle_duration, 60);
        assert_eq!(config.check_interval, 5);
    }
    
    #[test]
    fn test_idle_detector_creation() {
        let config = IdleConfig::default();
        let detector = IdleDetector::new(config);
        assert_eq!(detector.idle_duration(), 0);
    }
    
    #[test]
    fn test_get_metrics() {
        let config = IdleConfig::default();
        let mut detector = IdleDetector::new(config);
        
        let metrics = detector.get_metrics().unwrap();
        assert!(metrics.cpu_usage >= 0.0);
        assert!(metrics.memory_usage > 0);
    }
}

// Made with Bob
