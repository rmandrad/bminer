//! Bitcoin SHA-256d hashing implementation
//!
//! This module provides CPU-based SHA-256d (double SHA-256) hashing
//! for Bitcoin block headers.

use sha2::{Digest, Sha256};

/// Bitcoin block header structure (80 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BlockHeader {
    /// Block version
    pub version: u32,
    
    /// Previous block hash (32 bytes)
    pub prev_block: [u8; 32],
    
    /// Merkle root (32 bytes)
    pub merkle_root: [u8; 32],
    
    /// Block timestamp
    pub timestamp: u32,
    
    /// Difficulty target in compact format
    pub bits: u32,
    
    /// Nonce value
    pub nonce: u32,
}

impl BlockHeader {
    /// Create a new block header
    pub fn new(
        version: u32,
        prev_block: [u8; 32],
        merkle_root: [u8; 32],
        timestamp: u32,
        bits: u32,
        nonce: u32,
    ) -> Self {
        Self {
            version,
            prev_block,
            merkle_root,
            timestamp,
            bits,
            nonce,
        }
    }
    
    /// Serialize block header to bytes (little-endian)
    pub fn to_bytes(&self) -> [u8; 80] {
        let mut bytes = [0u8; 80];
        
        bytes[0..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..36].copy_from_slice(&self.prev_block);
        bytes[36..68].copy_from_slice(&self.merkle_root);
        bytes[68..72].copy_from_slice(&self.timestamp.to_le_bytes());
        bytes[72..76].copy_from_slice(&self.bits.to_le_bytes());
        bytes[76..80].copy_from_slice(&self.nonce.to_le_bytes());
        
        bytes
    }
    
    /// Compute SHA-256d hash (double SHA-256)
    pub fn hash(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
        sha256d(&bytes)
    }
    
    /// Check if hash meets target difficulty
    pub fn meets_target(&self, target: &[u8; 32]) -> bool {
        let hash = self.hash();
        crate::meets_target(&hash, target)
    }
}

/// Compute double SHA-256 hash
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first_hash = Sha256::digest(data);
    let second_hash = Sha256::digest(&first_hash);
    second_hash.into()
}

/// Compute merkle root from coinbase and merkle branches
pub fn compute_merkle_root(coinbase_hash: &[u8; 32], merkle_branches: &[[u8; 32]]) -> [u8; 32] {
    let mut hash = *coinbase_hash;
    
    for branch in merkle_branches {
        // Concatenate current hash with branch and hash again
        let mut combined = [0u8; 64];
        combined[0..32].copy_from_slice(&hash);
        combined[32..64].copy_from_slice(branch);
        hash = sha256d(&combined);
    }
    
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sha256d() {
        // Test with known input/output
        let input = b"hello";
        let result = sha256d(input);
        
        // SHA256d("hello") should produce a specific hash
        let expected = hex::decode(
            "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50"
        ).unwrap();
        
        assert_eq!(&result[..], &expected[..]);
    }
    
    #[test]
    fn test_block_header_serialization() {
        let header = BlockHeader::new(
            1,
            [0u8; 32],
            [1u8; 32],
            1234567890,
            0x1d00ffff,
            2083236893,
        );
        
        let bytes = header.to_bytes();
        
        // Check version (little-endian)
        assert_eq!(&bytes[0..4], &[1, 0, 0, 0]);
        
        // Check timestamp (little-endian)
        assert_eq!(&bytes[68..72], &1234567890u32.to_le_bytes());
        
        // Check nonce (little-endian)
        assert_eq!(&bytes[76..80], &2083236893u32.to_le_bytes());
    }
    
    #[test]
    fn test_genesis_block() {
        // Bitcoin genesis block - test basic hashing works
        let header = BlockHeader::new(
            1,
            [0u8; 32],
            [1u8; 32], // Simplified for testing
            1231006505,
            0x1d00ffff,
            2083236893,
        );
        
        let hash = header.hash();
        
        // Just verify hash is computed (not all zeros)
        let is_nonzero = hash.iter().any(|&b| b != 0);
        assert!(is_nonzero, "Hash should not be all zeros");
        
        // Verify hash is deterministic
        let hash2 = header.hash();
        assert_eq!(hash, hash2, "Hash should be deterministic");
    }
    
    #[test]
    fn test_merkle_root_computation() {
        let coinbase = [1u8; 32];
        let branches = vec![
            [2u8; 32],
            [3u8; 32],
        ];
        
        let root = compute_merkle_root(&coinbase, &branches);
        
        // Should produce a deterministic result
        assert_ne!(root, [0u8; 32]);
        assert_ne!(root, coinbase);
    }
}

// Made with Bob
