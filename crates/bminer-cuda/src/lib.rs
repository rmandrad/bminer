//! BMiner CUDA - CUDA integration and GPU management
//!
//! This crate provides CUDA bindings and GPU management functionality.

pub mod device;
pub mod hasher;

use thiserror::Error;

/// Result type for GPU operations
pub type Result<T> = std::result::Result<T, GpuError>;

/// Errors that can occur during GPU operations
#[derive(Error, Debug)]
pub enum GpuError {
    #[error("CUDA error: {0}")]
    CudaError(String),

    #[error("NVML error: {0}")]
    NvmlError(String),

    #[error("No compatible GPUs found")]
    NoGpusFound,

    #[error("Device {0} not found")]
    DeviceNotFound(usize),

    #[error("Kernel launch failed: {0}")]
    KernelLaunchFailed(String),
}

// Made with Bob
