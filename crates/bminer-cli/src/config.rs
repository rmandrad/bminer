//! Configuration management for BMiner
//!
//! This module handles loading and validating configuration from TOML files.

use bminer_pool::PoolConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Main BMiner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BminerConfig {
    /// Mining pool configuration
    pub pool: PoolConfig,

    /// GPU configuration
    pub gpu: GpuConfig,

    /// Idle detection configuration
    pub idle: IdleConfig,

    /// Monitoring configuration
    pub monitoring: MonitoringConfig,

    /// Logging configuration
    pub logging: LoggingConfig,
}

/// GPU configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    /// GPU device IDs to use (empty = all available)
    #[serde(default)]
    pub devices: Vec<usize>,

    /// Mining intensity (1-10, higher = more aggressive)
    #[serde(default = "default_intensity")]
    pub intensity: u8,

    /// Maximum GPU temperature (Celsius)
    #[serde(default = "default_max_temp")]
    pub max_temperature: u32,

    /// Maximum power usage (watts, 0 = no limit)
    #[serde(default)]
    pub max_power: u32,

    /// Threads per block for CUDA kernels
    #[serde(default = "default_threads_per_block")]
    pub threads_per_block: u32,

    /// Blocks per grid for CUDA kernels
    #[serde(default = "default_blocks_per_grid")]
    pub blocks_per_grid: u32,
}

fn default_intensity() -> u8 {
    5
}
fn default_max_temp() -> u32 {
    80
}
fn default_threads_per_block() -> u32 {
    256
}
fn default_blocks_per_grid() -> u32 {
    1024
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            devices: Vec::new(),
            intensity: default_intensity(),
            max_temperature: default_max_temp(),
            max_power: 0,
            threads_per_block: default_threads_per_block(),
            blocks_per_grid: default_blocks_per_grid(),
        }
    }
}

/// Idle detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleConfig {
    /// Enable idle detection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// CPU usage threshold (percentage)
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f32,

    /// Minimum idle duration before mining (seconds)
    #[serde(default = "default_min_idle")]
    pub min_idle_duration: u64,

    /// Check interval (seconds)
    #[serde(default = "default_check_interval")]
    pub check_interval: u64,

    /// GPU utilization threshold (percentage)
    #[serde(default = "default_gpu_threshold")]
    pub gpu_threshold: u32,
}

fn default_true() -> bool {
    true
}
fn default_cpu_threshold() -> f32 {
    20.0
}
fn default_min_idle() -> u64 {
    60
}
fn default_check_interval() -> u64 {
    5
}
fn default_gpu_threshold() -> u32 {
    10
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            cpu_threshold: default_cpu_threshold(),
            min_idle_duration: default_min_idle(),
            check_interval: default_check_interval(),
            gpu_threshold: default_gpu_threshold(),
        }
    }
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable performance monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Statistics update interval (seconds)
    #[serde(default = "default_stats_interval")]
    pub stats_interval: u64,

    /// Enable web dashboard
    #[serde(default)]
    pub web_dashboard: bool,

    /// Web dashboard port
    #[serde(default = "default_web_port")]
    pub web_port: u16,
}

fn default_stats_interval() -> u64 {
    30
}
fn default_web_port() -> u16 {
    8080
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            stats_interval: default_stats_interval(),
            web_dashboard: false,
            web_port: default_web_port(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log to file
    #[serde(default)]
    pub file: Option<String>,

    /// Log to console
    #[serde(default = "default_true")]
    pub console: bool,

    /// JSON format
    #[serde(default)]
    pub json: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
            console: default_true(),
            json: false,
        }
    }
}

impl BminerConfig {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: BminerConfig = toml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from a string
    pub fn from_str(s: &str) -> Result<Self> {
        let config: BminerConfig = toml::from_str(s)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate pool URL
        if self.pool.url.is_empty() {
            return Err(ConfigError::ValidationError(
                "Pool URL cannot be empty".to_string(),
            ));
        }

        // Validate pool username
        if self.pool.username.is_empty() {
            return Err(ConfigError::ValidationError(
                "Pool username cannot be empty".to_string(),
            ));
        }

        // Validate GPU intensity
        if self.gpu.intensity < 1 || self.gpu.intensity > 10 {
            return Err(ConfigError::ValidationError(
                "GPU intensity must be between 1 and 10".to_string(),
            ));
        }

        if self.gpu.threads_per_block == 0 {
            return Err(ConfigError::ValidationError(
                "GPU threads_per_block must be greater than 0".to_string(),
            ));
        }

        if self.gpu.blocks_per_grid == 0 {
            return Err(ConfigError::ValidationError(
                "GPU blocks_per_grid must be greater than 0".to_string(),
            ));
        }

        // Validate idle thresholds
        if self.idle.cpu_threshold < 0.0 || self.idle.cpu_threshold > 100.0 {
            return Err(ConfigError::ValidationError(
                "CPU threshold must be between 0 and 100".to_string(),
            ));
        }

        if self.idle.gpu_threshold > 100 {
            return Err(ConfigError::ValidationError(
                "GPU threshold must be between 0 and 100".to_string(),
            ));
        }

        if self.monitoring.enabled && self.monitoring.stats_interval == 0 {
            return Err(ConfigError::ValidationError(
                "Monitoring stats_interval must be greater than 0 when monitoring is enabled"
                    .to_string(),
            ));
        }

        if self.monitoring.web_dashboard && self.monitoring.web_port == 0 {
            return Err(ConfigError::ValidationError(
                "Monitoring web_port must be greater than 0 when web_dashboard is enabled"
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// Save configuration to a TOML file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(self).map_err(|e| {
            ConfigError::ValidationError(format!("Failed to serialize config: {}", e))
        })?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Create a default configuration
    pub fn default_with_pool(url: String, username: String) -> Self {
        Self {
            pool: PoolConfig {
                url,
                username,
                password: "x".to_string(),
                worker_name: "bminer".to_string(),
            },
            gpu: GpuConfig::default(),
            idle: IdleConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BminerConfig::default_with_pool(
            "stratum+tcp://pool.example.com:3333".to_string(),
            "wallet_address".to_string(),
        );

        assert!(config.validate().is_ok());
        assert_eq!(config.gpu.intensity, 5);
        assert_eq!(config.idle.cpu_threshold, 20.0);
    }

    #[test]
    fn test_config_validation() {
        let mut config = BminerConfig::default_with_pool(
            "stratum+tcp://pool.example.com:3333".to_string(),
            "wallet_address".to_string(),
        );

        // Valid config
        assert!(config.validate().is_ok());

        // Invalid intensity
        config.gpu.intensity = 11;
        assert!(config.validate().is_err());
        config.gpu.intensity = 5;

        // Invalid CPU threshold
        config.idle.cpu_threshold = 150.0;
        assert!(config.validate().is_err());
        config.idle.cpu_threshold = 20.0;

        // Invalid CUDA launch parameters
        config.gpu.threads_per_block = 0;
        assert!(config.validate().is_err());
        config.gpu.threads_per_block = 256;

        config.gpu.blocks_per_grid = 0;
        assert!(config.validate().is_err());
        config.gpu.blocks_per_grid = 1024;

        // Invalid monitoring settings
        config.monitoring.stats_interval = 0;
        assert!(config.validate().is_err());
        config.monitoring.stats_interval = 30;

        config.monitoring.web_dashboard = true;
        config.monitoring.web_port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = BminerConfig::default_with_pool(
            "stratum+tcp://pool.example.com:3333".to_string(),
            "wallet_address".to_string(),
        );

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: BminerConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.pool.url, parsed.pool.url);
        assert_eq!(config.gpu.intensity, parsed.gpu.intensity);
    }
}

// Made with Bob
