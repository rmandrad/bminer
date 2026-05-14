//! GPU-accelerated SHA-256d hasher using CUDA
//!
//! This module provides a CUDA-based implementation of the Hasher trait
//! for high-performance Bitcoin mining.

use bminer_core::{Hasher, MiningError, MiningResult, Result, Work};
use cudarc::{
    driver::{CudaDevice, CudaSlice, LaunchAsync, LaunchConfig},
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
    Cuda {
        device: Arc<CudaDevice>,
        buffers: Mutex<GpuBuffers>,
    },
    CpuFallback,
}

struct GpuBuffers {
    midstate_dev: CudaSlice<u32>,
    block1_prefix_dev: CudaSlice<u32>,
    target_dev: CudaSlice<u32>,
    found_count_dev: CudaSlice<u32>,
    found_nonces_dev: CudaSlice<u32>,
    found_hashes_dev: CudaSlice<u8>,
    found_count_host: [u32; 1],
    found_nonces_host: [u32; MAX_GPU_RESULTS],
    found_hashes_host: [u8; MAX_GPU_RESULTS * 32],
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
            Ok(Ok(())) => {
                match Self::allocate_gpu_buffers(&device) {
                    Ok(buffers) => (
                        ComputeBackend::Cuda {
                            device,
                            buffers: Mutex::new(buffers),
                        },
                        gpu_name,
                    ),
                    Err(reason) => {
                        let device_name = format!("{} [CPU fallback: {}]", gpu_name, reason);
                        (ComputeBackend::CpuFallback, device_name)
                    }
                }
            }
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

    fn allocate_gpu_buffers(device: &Arc<CudaDevice>) -> std::result::Result<GpuBuffers, String> {
        let midstate_dev = device
            .alloc_zeros::<u32>(8)
            .map_err(|e| format!("failed to allocate GPU midstate buffer: {}", e))?;
        let block1_prefix_dev = device
            .alloc_zeros::<u32>(3)
            .map_err(|e| format!("failed to allocate GPU block1 prefix buffer: {}", e))?;
        let target_dev = device
            .alloc_zeros::<u32>(8)
            .map_err(|e| format!("failed to allocate GPU target buffer: {}", e))?;
        let found_count_dev = device
            .alloc_zeros::<u32>(1)
            .map_err(|e| format!("failed to allocate GPU result counter: {}", e))?;
        let found_nonces_dev = device
            .alloc_zeros::<u32>(MAX_GPU_RESULTS)
            .map_err(|e| format!("failed to allocate GPU nonce buffer: {}", e))?;
        let found_hashes_dev = device
            .alloc_zeros::<u8>(MAX_GPU_RESULTS * 32)
            .map_err(|e| format!("failed to allocate GPU hash buffer: {}", e))?;

        Ok(GpuBuffers {
            midstate_dev,
            block1_prefix_dev,
            target_dev,
            found_count_dev,
            found_nonces_dev,
            found_hashes_dev,
            found_count_host: [0; 1],
            found_nonces_host: [0; MAX_GPU_RESULTS],
            found_hashes_host: [0; MAX_GPU_RESULTS * 32],
        })
    }

    fn record_hashes(&self, count: u32) {
        self.total_hashes
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    fn prepare_midstate_input(header: &[u8; 80]) -> ([u32; 8], [u32; 3]) {
        let mut state = [
            0x6a09e667u32,
            0xbb67ae85,
            0x3c6ef372,
            0xa54ff53a,
            0x510e527f,
            0x9b05688c,
            0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut block0 = [0u32; 16];
        for (idx, chunk) in header[..64].chunks_exact(4).enumerate() {
            block0[idx] = u32::from_be_bytes(chunk.try_into().expect("chunk size"));
        }
        sha256_compress_words(&mut state, &block0);

        let mut block1_prefix = [0u32; 3];
        for (idx, chunk) in header[64..76].chunks_exact(4).enumerate() {
            block1_prefix[idx] = u32::from_be_bytes(chunk.try_into().expect("chunk size"));
        }

        (state, block1_prefix)
    }

    fn prepare_target_words(target: &[u8; 32]) -> [u32; 8] {
        let mut words = [0u32; 8];
        for (idx, chunk) in target.chunks_exact(4).enumerate() {
            words[idx] = u32::from_be_bytes(chunk.try_into().expect("chunk size"));
        }
        words
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
        buffers: &Mutex<GpuBuffers>,
        work: &Work,
        start_nonce: u32,
        count: u32,
        extranonce2: &[u8],
    ) -> Result<Vec<MiningResult>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let header = self.build_header(work, extranonce2)?;
        let (midstate, block1_prefix) = Self::prepare_midstate_input(&header);
        let target_words = Self::prepare_target_words(&work.target);
        let mut buffers = buffers
            .lock()
            .map_err(|_| MiningError::GpuError("GPU buffer lock poisoned".to_string()))?;
        let GpuBuffers {
            midstate_dev,
            block1_prefix_dev,
            target_dev,
            found_count_dev,
            found_nonces_dev,
            found_hashes_dev,
            found_count_host,
            found_nonces_host,
            found_hashes_host,
        } = &mut *buffers;

        device
            .htod_sync_copy_into(&midstate, midstate_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to copy midstate to GPU: {}", e)))?;
        device
            .htod_sync_copy_into(&block1_prefix, block1_prefix_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to copy block1 prefix to GPU: {}", e)))?;
        device
            .htod_sync_copy_into(&target_words, target_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to copy target to GPU: {}", e)))?;
        device
            .memset_zeros(found_count_dev)
            .map_err(|e| MiningError::GpuError(format!("Failed to reset GPU result counter: {}", e)))?;

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
                        &*midstate_dev,
                        &*block1_prefix_dev,
                        &*target_dev,
                        start_nonce,
                        count,
                        MAX_GPU_RESULTS as u32,
                        &mut *found_count_dev,
                        &mut *found_nonces_dev,
                        &mut *found_hashes_dev,
                    ),
                )
                .map_err(|e| MiningError::GpuError(format!("Failed to launch CUDA mining kernel: {}", e)))?;
        }

        device
            .dtoh_sync_copy_into(&*found_count_dev, found_count_host)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU result counter: {}", e)))?;
        let found_count = found_count_host[0]
            .min(MAX_GPU_RESULTS as u32) as usize;

        self.record_hashes(count);

        if found_count == 0 {
            return Ok(Vec::new());
        }

        device
            .dtoh_sync_copy_into(&*found_nonces_dev, found_nonces_host)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU nonces: {}", e)))?;
        device
            .dtoh_sync_copy_into(&*found_hashes_dev, found_hashes_host)
            .map_err(|e| MiningError::GpuError(format!("Failed to read GPU hashes: {}", e)))?;

        let mut results = Vec::with_capacity(found_count);
        for slot in 0..found_count {
            let mut hash = [0u8; 32];
            let offset = slot * 32;
            hash.copy_from_slice(&found_hashes_host[offset..offset + 32]);

            results.push(MiningResult {
                nonce: found_nonces_host[slot],
                hash,
                meets_target: true,
                extranonce2: extranonce2.to_vec(),
            });
        }

        results.sort_by_key(|result| result.nonce);
        Ok(results)
    }
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[inline]
fn sha256_compress_words(state: &mut [u32; 8], block: &[u32; 16]) {
    let mut w = [0u32; 64];
    w[..16].copy_from_slice(block);

    for idx in 16..64 {
        let s0 = w[idx - 15].rotate_right(7) ^ w[idx - 15].rotate_right(18) ^ (w[idx - 15] >> 3);
        let s1 = w[idx - 2].rotate_right(17) ^ w[idx - 2].rotate_right(19) ^ (w[idx - 2] >> 10);
        w[idx] = w[idx - 16]
            .wrapping_add(s0)
            .wrapping_add(w[idx - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for idx in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[idx])
            .wrapping_add(w[idx]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
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
            ComputeBackend::Cuda { device, buffers } => match self.mine_gpu(device, buffers, work, start_nonce, count, extranonce2) {
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
    use bminer_core::hasher::BlockHeader;

    fn hash_from_midstate(header: &[u8; 80]) -> [u8; 32] {
        let (mut first_state, block1_prefix) = CudaHasher::prepare_midstate_input(header);

        let mut block1 = [0u32; 16];
        block1[..3].copy_from_slice(&block1_prefix);
        block1[3] = u32::from_be_bytes(header[76..80].try_into().expect("nonce chunk"));
        block1[4] = 0x80000000;
        block1[15] = 0x00000280;
        sha256_compress_words(&mut first_state, &block1);

        let mut second_state = [
            0x6a09e667u32,
            0xbb67ae85,
            0x3c6ef372,
            0xa54ff53a,
            0x510e527f,
            0x9b05688c,
            0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut second_block = [0u32; 16];
        second_block[..8].copy_from_slice(&first_state);
        second_block[8] = 0x80000000;
        second_block[15] = 0x00000100;
        sha256_compress_words(&mut second_state, &second_block);

        let mut hash = [0u8; 32];
        for (idx, word) in second_state.into_iter().enumerate() {
            hash[idx * 4..(idx + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        hash
    }
    
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

    #[test]
    fn midstate_path_matches_full_sha256d() {
        let header = BlockHeader::new(
            0x20000000,
            [0x11; 32],
            [0x22; 32],
            0x5f5e100,
            0x170d5c5a,
            0x12345678,
        )
        .to_bytes();

        let expected = bminer_core::hasher::sha256d(&header);
        let actual = hash_from_midstate(&header);

        assert_eq!(actual, expected);
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
