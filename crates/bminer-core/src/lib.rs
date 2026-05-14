//! BMiner Core - Core mining logic and algorithms
//!
//! This crate provides the fundamental types, traits, and algorithms for Bitcoin mining.

pub mod hasher;

use thiserror::Error;

/// Result type for mining operations
pub type Result<T> = std::result::Result<T, MiningError>;

/// Errors that can occur during mining operations
#[derive(Error, Debug)]
pub enum MiningError {
    #[error("Invalid block header: {0}")]
    InvalidHeader(String),
    
    #[error("GPU error: {0}")]
    GpuError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Represents a mining work unit from the pool
#[derive(Debug, Clone)]
pub struct Work {
    /// Job identifier from the pool
    pub job_id: String,
    
    /// Previous block hash (32 bytes)
    pub prev_hash: [u8; 32],
    
    /// Coinbase transaction
    pub coinbase: Vec<u8>,
    
    /// Merkle branch hashes for building merkle root
    pub merkle_branches: Vec<[u8; 32]>,
    
    /// Block version
    pub version: u32,
    
    /// Difficulty target in compact format
    pub nbits: u32,
    
    /// Block timestamp
    pub ntime: u32,
    
    /// Target difficulty (full 256-bit)
    pub target: [u8; 32],
    
    /// Extra nonce 1 from pool
    pub extranonce1: Vec<u8>,
    
    /// Extra nonce 2 size
    pub extranonce2_size: usize,
}

impl Work {
    /// Create a new work unit
    pub fn new(
        job_id: String,
        prev_hash: [u8; 32],
        coinbase: Vec<u8>,
        merkle_branches: Vec<[u8; 32]>,
        version: u32,
        nbits: u32,
        ntime: u32,
    ) -> Self {
        let target = bits_to_target(nbits);
        
        Self {
            job_id,
            prev_hash,
            coinbase,
            merkle_branches,
            version,
            nbits,
            ntime,
            target,
            extranonce1: Vec::new(),
            extranonce2_size: 0,
        }
    }
}

/// Result of mining computation
#[derive(Debug, Clone)]
pub struct MiningResult {
    /// Nonce that was tested
    pub nonce: u32,
    
    /// Resulting hash
    pub hash: [u8; 32],
    
    /// Whether this hash meets the target difficulty
    pub meets_target: bool,
    
    /// Extra nonce 2 value used
    pub extranonce2: Vec<u8>,
}

/// Trait for hash computation engines
#[async_trait::async_trait]
pub trait Hasher: Send + Sync {
    /// Compute hashes for a range of nonces
    async fn hash_range(
        &self,
        work: &Work,
        start_nonce: u32,
        count: u32,
        extranonce2: &[u8],
    ) -> Result<Vec<MiningResult>>;
    
    /// Get the hasher's performance in hashes per second
    fn hashrate(&self) -> f64;
    
    /// Get the device name/identifier
    fn device_name(&self) -> String;
}

/// Convert compact bits format to full 256-bit target
pub fn bits_to_target(bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    
    let exponent = (bits >> 24) as usize;
    let mantissa = bits & 0x00ffffff;
    
    // Convert mantissa to big-endian bytes
    let mantissa_bytes = mantissa.to_be_bytes();
    
    if exponent <= 3 {
        // For small exponents, place mantissa at the start
        let len = exponent.min(3);
        target[32 - len..32].copy_from_slice(&mantissa_bytes[4 - len..4]);
    } else if exponent <= 32 {
        // Place mantissa at position determined by exponent
        let start = 32 - exponent;
        if start + 3 <= 32 {
            target[start..start + 3].copy_from_slice(&mantissa_bytes[1..4]);
        }
    }
    
    target
}

/// Check if a hash meets the target difficulty
pub fn meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    // Compare in reverse order (big-endian comparison)
    for i in (0..32).rev() {
        if hash[i] < target[i] {
            return true;
        } else if hash[i] > target[i] {
            return false;
        }
    }
    
    true // Equal to target
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bits_to_target() {
        // Test case from Bitcoin genesis block
        let bits = 0x1d00ffff;
        let target = bits_to_target(bits);
        
        // The target should be non-zero
        let is_nonzero = target.iter().any(|&b| b != 0);
        assert!(is_nonzero, "Target should not be all zeros");
        
        // Should have leading zeros (high difficulty)
        assert_eq!(target[0], 0x00, "Should have leading zeros");
    }
    
    #[test]
    fn test_meets_target() {
        // Test with maximum target (easiest difficulty)
        let max_target = [0xff; 32];
        let any_hash = [0xaa; 32];
        assert!(meets_target(&any_hash, &max_target), "Any hash should meet max target");
        
        // Test with zero hash always meets any target
        let zero_hash = [0x00; 32];
        assert!(meets_target(&zero_hash, &max_target), "Zero hash should meet any target");
        
        // Test equal hashes
        let hash = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
                    0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
                    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert!(meets_target(&hash, &hash), "Equal hash and target should pass");
    }
    
    #[test]
    fn test_work_creation() {
        let work = Work::new(
            "test_job".to_string(),
            [0u8; 32],
            vec![0u8; 100],
            vec![],
            1,
            0x1d00ffff,
            1234567890,
        );
        
        assert_eq!(work.job_id, "test_job");
        assert_eq!(work.version, 1);
        assert_eq!(work.ntime, 1234567890);
    }
}

// Made with Bob
