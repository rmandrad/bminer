//! BMiner Pool - Mining pool protocol implementation
//!
//! This crate provides Stratum protocol implementation for pool mining.

pub mod stratum;

use bminer_core::{MiningError, Work};
use thiserror::Error;

/// Result type for pool operations
pub type Result<T> = std::result::Result<T, PoolError>;

/// Errors that can occur during pool operations
#[derive(Error, Debug)]
pub enum PoolError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Mining error: {0}")]
    MiningError(#[from] MiningError),
}

/// Pool configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolConfig {
    /// Pool URL (e.g., "stratum+tcp://pool.example.com:3333")
    pub url: String,

    /// Username (typically wallet address)
    pub username: String,

    /// Password (often just "x")
    pub password: String,

    /// Worker name
    pub worker_name: String,
}

/// Share submission
#[derive(Debug, Clone)]
pub struct Share {
    /// Job ID
    pub job_id: String,

    /// Nonce value
    pub nonce: u32,

    /// Block timestamp
    pub ntime: u32,

    /// Extra nonce 2
    pub extranonce2: Vec<u8>,
}

/// Trait for pool client implementations
#[async_trait::async_trait]
pub trait PoolClient: Send + Sync {
    /// Connect to the mining pool
    async fn connect(&mut self) -> Result<()>;

    /// Subscribe to mining notifications
    async fn subscribe(&mut self) -> Result<()>;

    /// Authorize worker
    async fn authorize(&mut self) -> Result<()>;

    /// Receive new work from pool
    async fn receive_work(&mut self) -> Result<Work>;

    /// Submit a share to the pool
    async fn submit_share(&mut self, share: Share) -> Result<bool>;

    /// Disconnect from pool
    async fn disconnect(&mut self) -> Result<()>;
}

// Made with Bob
