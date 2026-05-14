//! BMiner Monitor - System and GPU monitoring
//!
//! This crate provides system monitoring and idle detection functionality.

pub mod idle;

use thiserror::Error;

/// Result type for monitoring operations
pub type Result<T> = std::result::Result<T, MonitorError>;

/// Errors that can occur during monitoring
#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("System error: {0}")]
    SystemError(String),
    
    #[error("GPU monitoring error: {0}")]
    GpuError(String),
}

/// System metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_usage: f32,
    
    /// Memory usage in bytes
    pub memory_usage: u64,
    
    /// Timestamp of measurement
    pub timestamp: std::time::Instant,
}

/// GPU metrics
#[derive(Debug, Clone)]
pub struct GpuMetrics {
    /// Device ID
    pub device_id: usize,
    
    /// Temperature in Celsius
    pub temperature: u32,
    
    /// Power usage in milliwatts
    pub power_usage: u32,
    
    /// GPU utilization percentage (0-100)
    pub utilization: u32,
    
    /// Memory used in bytes
    pub memory_used: u64,
    
    /// Fan speed percentage (0-100)
    pub fan_speed: u32,
}

/// Trait for system monitoring
#[async_trait::async_trait]
pub trait SystemMonitor: Send + Sync {
    /// Check if system is idle
    async fn is_idle(&self) -> Result<bool>;
    
    /// Get current system metrics
    async fn system_metrics(&self) -> Result<SystemMetrics>;
    
    /// Get GPU metrics for all devices
    async fn gpu_metrics(&self) -> Result<Vec<GpuMetrics>>;
}

// Made with Bob
