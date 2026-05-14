//! GPU-accelerated SHA-256d hasher using CUDA
//!
//! This module provides a CUDA-based implementation of the Hasher trait
//! for high-performance Bitcoin mining.

use bminer_core::{Hasher, MiningError, MiningResult, Result, Work};
use cudarc::{
    driver::{CudaDevice, LaunchAsync, LaunchConfig},
    nvrtc::compile_ptx,
};
use std::any::Any;
use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe, PanicHookInfo};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Instant;

const CUDA_MODULE_NAME: &str = "bminer_sha256d";
const CUDA_KERNEL_NAME: &str = "mine_sha256d";
const CUDA_KERNEL_SRC: &str = include_str!("sha256d_kernel.cu");
const MAX_GPU_RESULTS: usize = 32;
static PANIC_HOOK_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

enum ComputeBackend {
    Cuda { device: Arc<CudaDevice> },
    CpuFallback,
}

/// GPU-accelerated hasher using CUDA
pub struct CudaHasher {
    backend: ComputeBackend,
    device_name: String,
    threads_per_block: u32,
    blocks_per_grid: u32,
    total_hashes: AtomicU64,
    start_time: Instant,
}

impl CudaHasher {
    const DEFAULT_THREADS_PER_BLOCK: u32 = 256;
    const DEFAULT_BLOCKS_PER_GRID: u32 = 1024;

    /// Create a new CUDA hasher for the specified device
    pub fn new(device_id: usize) -> Result<Self> {
        Self::new_with_launch_params(
            device_id,
            Self::DEFAULT_THREADS_PER_BLOCK,
            Self::DEFAULT_BLOCKS_PER_GRID,
        )
    }

    /// Create a new CUDA hasher with explicit launch parameters.
    pub fn new_with_launch_params(
        device_id: usize,
        threads_per_block: u32,
        blocks_per_grid: u32,
    ) -> Result<Self> {
        let (backend, device_name) = Self::initialize_backend(device_id);

        Ok(Self {
            backend,
            device_name,
            threads_per_block,
            blocks_per_grid,
            total_hashes: AtomicU64::new(0),
            start_time: Instant::now(),
        })
    }
    
    /// Set kernel launch parameters
    pub fn set_launch_params(&mut self, threads_per_block: u32, blocks_per_grid: u32) {
        self.threads_per_block = threads_per_block;
        self.blocks_per_grid = blocks_per_grid;
    }

    /// Returns true when the hasher is actively using the CUDA backend.
    pub fn is_cuda_backend(&self) -> bool {
        matches!(self.backend, ComputeBackend::Cuda { .. })
    }

    fn initialize_backend(device_id: usize) -> (ComputeBackend, String) {
        let device = match Self::catch_probe_panic(|| CudaDevice::new(device_id)) {
            Ok(Ok(device)) => device,
            Ok(Err(err)) => {
                let reason = format!("CUDA unavailable for device {}: {}", device_id, err);
                let device_name = format!("CPU fallback [{}]", reason);
                return (ComputeBackend::CpuFallback, device_name);
            }
            Err(panic) => {
                let reason = format!(
                    "CUDA unavailable for device {}: {}",
                    device_id,
                    Self::panic_message(panic)
                );
                let device_name = format!("CPU fallback [{}]", reason);
                return (ComputeBackend::CpuFallback, device_name);
            }
        };

        let gpu_name = device
            .name()
            .unwrap_or_else(|_| format!("CUDA Device {}", device_id));

        match Self::catch_probe_panic(|| Self::load_mining_kernel(&device)) {
            Ok(Ok(())) => (ComputeBackend::Cuda { device }, gpu_name),
            Ok(Err(reason)) => {
                let device_name = format!("{} [CPU fallback: {}]", gpu_name, reason);
                (ComputeBackend::CpuFallback, device_name)
            }
            Err(panic) => {
                let reason = format!("failed to initialize CUDA kernel: {}", Self::panic_message(panic));
                let device_name = format!("{} [CPU fallback: {}]", gpu_name, reason);
                (ComputeBackend::CpuFallback, device_name)
            }
        }
    }

    fn catch_probe_panic<F, T>(probe: F) -> std::result::Result<T, Box<dyn Any + Send>>
    where
        F: FnOnce() -> T,
    {
        let guard = PANIC_HOOK_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("panic hook guard poisoned");
        let previous_hook = take_hook();
        set_hook(Box::new(|_: &PanicHookInfo<'_>| {}));
        let result = catch_unwind(AssertUnwindSafe(probe));
        set_hook(previous_hook);
        drop(guard);
        result
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

    fn load_mining_kernel(device: &Arc<CudaDevice>) -> std::result::Result<(), String> {
        let ptx = compile_ptx(CUDA_KERNEL_SRC)
            .map_err(|e| format!("failed to compile CUDA kernel: {}", e))?;

        device
            .load_ptx(ptx, CUDA_MODULE_NAME, &[CUDA_KERNEL_NAME])
            .map_err(|e| format!("failed to load CUDA kernel module: {}", e))
    }

    fn record_hashes(&self, count: u32) {
        self.total_hashes
            .fetch_add(count as u64, Ordering::Relaxed);
    }
    
    /// Build block header from work
    fn build_header(&self, work: &Work, extranonce2: &[u8]) -> Result<[u8; 80]> {
        use bminer_core::hasher::{compute_merkle_root, sha256d};
        
        let mut header = [0u8; 80];
        
        // Version (4 bytes, little-endian)
        header[0..4].copy_from_slice(&work.version.to_le_bytes());
        
        // Previous block hash (32 bytes)
        header[4..36].copy_from_slice(&work.prev_hash);
        
        // Build coinbase transaction with extranonces
        let mut coinbase = work.coinbase.clone();
        coinbase.extend_from_slice(&work.extranonce1);
        coinbase.extend_from_slice(extranonce2);
        
        // Compute coinbase hash
        let coinbase_hash = sha256d(&coinbase);
        
        // Compute merkle root
        let merkle_root = compute_merkle_root(&coinbase_hash, &work.merkle_branches);
        header[36..68].copy_from_slice(&merkle_root);
        
        // Timestamp (4 bytes, little-endian)
        header[68..72].copy_from_slice(&work.ntime.to_le_bytes());
        
        // Bits (4 bytes, little-endian)
        header[72..76].copy_from_slice(&work.nbits.to_le_bytes());
        
        // Nonce will be set by the kernel (4 bytes)
        // header[76..80] left as zeros for now
        
        Ok(header)
    }
    
    /// Mine using CPU (fallback implementation)
    async fn mine_cpu(
        &self,
        work: &Work,
        start_nonce: u32,
        count: u32,
        extranonce2: &[u8],
    ) -> Result<Vec<MiningResult>> {
        use bminer_core::hasher::BlockHeader;
        
        let header = self.build_header(work, extranonce2)?;
        let mut results = Vec::new();
        
        // Parse header into BlockHeader struct
        let version = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let mut prev_block = [0u8; 32];
        prev_block.copy_from_slice(&header[4..36]);
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&header[36..68]);
        let timestamp = u32::from_le_bytes([header[68], header[69], header[70], header[71]]);
        let bits = u32::from_le_bytes([header[72], header[73], header[74], header[75]]);
        
        // Test each nonce
        for i in 0..count {
            let nonce = start_nonce.wrapping_add(i);
            
            let block_header = BlockHeader::new(
                version,
                prev_block,
                merkle_root,
                timestamp,
                bits,
                nonce,
            );
            
            let hash = block_header.hash();
            let meets_target = block_header.meets_target(&work.target);
            
            if meets_target {
                results.push(MiningResult {
                    nonce,
                    hash,
                    meets_target: true,
                    extranonce2: extranonce2.to_vec(),
                });
                break;
            }
        }

        self.record_hashes(count);
        
        Ok(results)
    }

    fn mine_gpu(
        &self,
        device: &Arc<CudaDevice>,
        work: &Work,
        start_nonce: u32,
        count: u32,
        extranonce2: &[u8],
    ) -> Result<Vec<MiningResult>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let header = self.build_header(work, extranonce2)?;
        let header_dev = device
            .htod_copy(header.to_vec())
            .map_err(|e| MiningError::GpuError(format!("Failed to copy header to GPU: {}", e)))?;
        let target_dev = device
            .htod_copy(work.target.to_vec())
            .map_err(|e| MiningError::GpuError(format!("Failed to copy target to GPU: {}", e)))?;
        let mut found_count_dev = device
            .alloc_zeros::<u32>(1)
            .map_err(|e| MiningError::GpuError(format!("Failed to allocate GPU result counter: {}", e)))?;
        let mut found_nonces_dev = device
            .alloc_zeros::<u32>(MAX_GPU_RESULTS)
            .map_err(|e| MiningError::GpuError(format!("Failed to allocate GPU nonce buffer: {}", e)))?;
        let mut found_hashes_dev = device
            .alloc_zeros::<u8>(MAX_GPU_RESULTS * 32)
            .map_err(|e| MiningError::GpuError(format!("Failed to allocate GPU hash buffer: {}", e)))?;

        let kernel = device
            .get_func(CUDA_MODULE_NAME, CUDA_KERNEL_NAME)
            .ok_or_else(|| MiningError::GpuError("CUDA mining kernel was not loaded".to_string()))?;

        let config = LaunchConfig {
            grid_dim: (self.blocks_per_grid, 1, 1),
            block_dim: (self.threads_per_block, 1, 1),
            shared_mem_bytes: 0,
        };

        unsafe {
            kernel
                .launch(
                    config,
                    (
                        &header_dev,
                        &target_dev,
                        start_nonce,
                        count,
                        MAX_GPU_RESULTS as u32,
                        &mut found_count_dev,
                        &mut found_nonces_dev,
                        &mut found_hashes_dev,
                    ),
                )
                .map_err(|e| MiningError::GpuError(format!("Failed to launch CUDA mining kernel: {}", e)))?;
        }

        let found_count = device
            .dtoh_sync_copy(&found_count_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU result counter: {}", e)))?[0]
            .min(MAX_GPU_RESULTS as u32) as usize;

        self.record_hashes(count);

        if found_count == 0 {
            return Ok(Vec::new());
        }

        let found_nonces = device
            .dtoh_sync_copy(&found_nonces_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU nonces: {}", e)))?;
        let found_hashes = device
            .dtoh_sync_copy(&found_hashes_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU hashes: {}", e)))?;

        let mut results = Vec::with_capacity(found_count);
        for slot in 0..found_count {
            let mut hash = [0u8; 32];
            let offset = slot * 32;
            hash.copy_from_slice(&found_hashes[offset..offset + 32]);

            results.push(MiningResult {
                nonce: found_nonces[slot],
                hash,
                meets_target: true,
                extranonce2: extranonce2.to_vec(),
            });
        }

        results.sort_by_key(|result| result.nonce);
        Ok(results)
    }
}

#[async_trait::async_trait]
impl Hasher for CudaHasher {
    async fn hash_range(
        &self,
        work: &Work,
        start_nonce: u32,
        count: u32,
        extranonce2: &[u8],
    ) -> Result<Vec<MiningResult>> {
        match &self.backend {
            ComputeBackend::Cuda { device } => match self.mine_gpu(device, work, start_nonce, count, extranonce2) {
                Ok(results) => Ok(results),
                Err(_) => self.mine_cpu(work, start_nonce, count, extranonce2).await,
            },
            ComputeBackend::CpuFallback => self.mine_cpu(work, start_nonce, count, extranonce2).await,
        }
    }
    
    fn hashrate(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_hashes.load(Ordering::Relaxed) as f64 / elapsed
        } else {
            0.0
        }
    }
    
    fn device_name(&self) -> String {
        self.device_name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn falls_back_to_cpu_when_cuda_is_unavailable() {
        let hasher = CudaHasher::new_with_launch_params(usize::MAX, 256, 1024).unwrap();

        assert!(!hasher.is_cuda_backend());
        assert!(hasher.device_name().contains("CPU fallback"));
    }

    #[tokio::test]
    async fn cpu_fallback_still_hashes_work() {
        let hasher = CudaHasher::new_with_launch_params(usize::MAX, 256, 1024).unwrap();
        let mut work = Work::new(
            "test".to_string(),
            [0u8; 32],
            vec![0u8; 100],
            vec![],
            1,
            0x1d00ffff,
            1234567890,
        );
        work.target = [0xff; 32];

        let results = hasher.hash_range(&work, 0, 16, &[]).await.unwrap();

        assert!(!results.is_empty());
        assert!(results[0].meets_target);
    }

    #[tokio::test]
    #[ignore] // Requires NVIDIA GPU
    async fn test_cuda_hasher() {
        let hasher = CudaHasher::new(0);
        
        if let Ok(hasher) = hasher {
            let work = Work::new(
                "test".to_string(),
                [0u8; 32],
                vec![0u8; 100],
                vec![],
                1,
                0x1d00ffff,
                1234567890,
            );
            
            let results = hasher.hash_range(&work, 0, 1000, &[]).await.unwrap();
            assert!(!results.is_empty());
            
            println!("Hashrate: {:.2} H/s", hasher.hashrate());
        }
    }
}

// Made with Bob
