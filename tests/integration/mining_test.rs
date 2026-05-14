//! Integration tests for BMiner
//!
//! These tests verify the end-to-end functionality of the mining system.

use bminer_core::{bits_to_target, meets_target, Work};
use bminer_core::hasher::{BlockHeader, sha256d};

#[test]
fn test_genesis_block_hash() {
    // Bitcoin genesis block
    let header = BlockHeader::new(
        1,
        [0u8; 32],
        hex::decode("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b")
            .unwrap()
            .try_into()
            .unwrap(),
        1231006505,
        0x1d00ffff,
        2083236893,
    );
    
    let hash = header.hash();
    
    // Genesis block hash (reversed for display)
    let expected = hex::decode("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f")
        .unwrap();
    
    let mut hash_reversed = hash;
    hash_reversed.reverse();
    
    assert_eq!(&hash_reversed[..], &expected[..], "Genesis block hash mismatch");
}

#[test]
fn test_sha256d_known_value() {
    let input = b"hello world";
    let result = sha256d(input);
    
    // Known SHA256d("hello world")
    let expected = hex::decode("bc62d4b80d9e36da29c16c5d4d9f11731f36052c72401a76c23c0fb5a9b74423")
        .unwrap();
    
    assert_eq!(&result[..], &expected[..], "SHA256d hash mismatch");
}

#[test]
fn test_bits_to_target_conversion() {
    // Test various difficulty targets
    let test_cases = vec![
        (0x1d00ffff, 26), // Genesis difficulty
        (0x1b0404cb, 23), // Higher difficulty
        (0x17148edf, 20), // Even higher
    ];
    
    for (bits, expected_leading_zeros) in test_cases {
        let target = bits_to_target(bits);
        
        // Count leading zero bytes
        let leading_zeros = target.iter().rev().take_while(|&&b| b == 0).count();
        
        assert!(
            leading_zeros >= expected_leading_zeros,
            "Bits {:08x} should have at least {} leading zero bytes, got {}",
            bits, expected_leading_zeros, leading_zeros
        );
    }
}

#[test]
fn test_target_comparison() {
    let target = [0xff; 32];
    
    // Hash below target should pass
    let hash_below = [0xfe; 32];
    assert!(meets_target(&hash_below, &target), "Hash below target should meet target");
    
    // Hash equal to target should pass
    let hash_equal = [0xff; 32];
    assert!(meets_target(&hash_equal, &target), "Hash equal to target should meet target");
    
    // Hash above target should fail
    let mut hash_above = [0xff; 32];
    hash_above[31] = 0x01; // Make it slightly higher
    assert!(!meets_target(&hash_above, &target), "Hash above target should not meet target");
}

#[test]
fn test_work_creation() {
    let work = Work::new(
        "test_job_123".to_string(),
        [1u8; 32],
        vec![0u8; 100],
        vec![[2u8; 32], [3u8; 32]],
        1,
        0x1d00ffff,
        1234567890,
    );
    
    assert_eq!(work.job_id, "test_job_123");
    assert_eq!(work.version, 1);
    assert_eq!(work.ntime, 1234567890);
    assert_eq!(work.nbits, 0x1d00ffff);
    assert_eq!(work.merkle_branches.len(), 2);
    assert_ne!(work.target, [0u8; 32], "Target should be computed");
}

#[test]
fn test_block_header_serialization() {
    let header = BlockHeader::new(
        0x20000000,
        [0xaa; 32],
        [0xbb; 32],
        1609459200,
        0x1d00ffff,
        12345678,
    );
    
    let bytes = header.to_bytes();
    
    // Verify length
    assert_eq!(bytes.len(), 80, "Block header should be 80 bytes");
    
    // Verify version (little-endian)
    assert_eq!(&bytes[0..4], &0x20000000u32.to_le_bytes());
    
    // Verify prev_block
    assert_eq!(&bytes[4..36], &[0xaa; 32]);
    
    // Verify merkle_root
    assert_eq!(&bytes[36..68], &[0xbb; 32]);
    
    // Verify timestamp (little-endian)
    assert_eq!(&bytes[68..72], &1609459200u32.to_le_bytes());
    
    // Verify bits (little-endian)
    assert_eq!(&bytes[72..76], &0x1d00ffffu32.to_le_bytes());
    
    // Verify nonce (little-endian)
    assert_eq!(&bytes[76..80], &12345678u32.to_le_bytes());
}

#[test]
fn test_difficulty_target_ordering() {
    // Lower bits value = higher difficulty = lower target
    let easy_bits = 0x1d00ffff;
    let hard_bits = 0x1b0404cb;
    
    let easy_target = bits_to_target(easy_bits);
    let hard_target = bits_to_target(hard_bits);
    
    // Hard target should be numerically smaller
    let easy_nonzero = easy_target.iter().position(|&b| b != 0).unwrap();
    let hard_nonzero = hard_target.iter().position(|&b| b != 0).unwrap();
    
    assert!(
        hard_nonzero > easy_nonzero,
        "Harder difficulty should have more leading zeros"
    );
}

#[cfg(test)]
mod pool_tests {
    use bminer_pool::PoolConfig;
    
    #[test]
    fn test_pool_config_creation() {
        let config = PoolConfig {
            url: "stratum+tcp://pool.example.com:3333".to_string(),
            username: "test_wallet".to_string(),
            password: "x".to_string(),
            worker_name: "test_worker".to_string(),
        };
        
        assert!(config.url.starts_with("stratum+tcp://"));
        assert!(!config.username.is_empty());
    }
}

#[cfg(test)]
mod monitor_tests {
    use bminer_monitor::idle::IdleConfig;
    
    #[test]
    fn test_idle_config_defaults() {
        let config = IdleConfig::default();
        
        assert_eq!(config.cpu_threshold, 20.0);
        assert_eq!(config.min_idle_duration, 60);
        assert_eq!(config.check_interval, 5);
        assert_eq!(config.gpu_threshold, 10);
    }
    
    #[test]
    fn test_idle_config_custom() {
        let config = IdleConfig {
            cpu_threshold: 30.0,
            min_idle_duration: 120,
            check_interval: 10,
            gpu_threshold: 15,
        };
        
        assert_eq!(config.cpu_threshold, 30.0);
        assert_eq!(config.min_idle_duration, 120);
    }
}

// Made with Bob
