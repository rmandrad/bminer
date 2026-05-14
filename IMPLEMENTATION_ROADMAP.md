# BMiner Implementation Roadmap

## Overview

This document provides detailed implementation guidance for building BMiner, a production-ready Bitcoin GPU miner in Rust. Each phase includes specific tasks, code examples, and validation criteria.

## Phase 1: Foundation (Week 1-2)

### Task 1: Project Initialization and Setup

**Objective**: Create the Cargo workspace structure with all necessary dependencies.

**Steps**:

1. **Initialize Workspace**
```bash
cargo new --lib bminer
cd bminer
mkdir -p crates/{bminer-core,bminer-cuda,bminer-pool,bminer-monitor,bminer-cli}
mkdir -p config docs tests/{integration,benchmarks}
```

2. **Create Root Cargo.toml**
```toml
[workspace]
resolver = "2"
members = [
    "crates/bminer-core",
    "crates/bminer-cuda",
    "crates/bminer-pool",
    "crates/bminer-monitor",
    "crates/bminer-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Your Name <your.email@example.com>"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/yourusername/bminer"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.40", features = ["full"] }
async-trait = "0.1"

# CUDA bindings
cudarc = { version = "0.11", features = ["cuda-12050"] }

# Cryptography
sha2 = "0.10"
hex = "0.4"

# Networking
tokio-tungstenite = "0.23"
serde_json = "1.0"

# Configuration
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
config = "0.14"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# System monitoring
sysinfo = "0.31"
nvml-wrapper = "0.10"

# CLI
clap = { version = "4.5", features = ["derive"] }

# Utilities
bytes = "1.7"
futures = "0.3"

# Testing
criterion = "0.5"
mockall = "0.13"
```

3. **Create Individual Crate Manifests**

Each crate needs its own `Cargo.toml`. Example for `bminer-core`:
```toml
[package]
name = "bminer-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
sha2.workspace = true
hex.workspace = true
thiserror.workspace = true
serde.workspace = true
bytes.workspace = true

[dev-dependencies]
criterion.workspace = true
```

4. **Setup Git and CI**
```bash
git init
echo "target/" > .gitignore
echo "Cargo.lock" >> .gitignore
echo "*.log" >> .gitignore
```

**Validation**:
- `cargo build` succeeds for all crates
- Workspace structure is correct
- All dependencies resolve

---

### Task 2: Core Architecture Design

**Objective**: Define module structure, traits, and interfaces.

**Key Traits**:

1. **Hasher Trait** (`bminer-core/src/lib.rs`)
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MiningError {
    #[error("Invalid block header: {0}")]
    InvalidHeader(String),
    #[error("GPU error: {0}")]
    GpuError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}

pub type Result<T> = std::result::Result<T, MiningError>;

/// Represents a mining work unit
#[derive(Debug, Clone)]
pub struct Work {
    pub job_id: String,
    pub prev_hash: [u8; 32],
    pub coinbase: Vec<u8>,
    pub merkle_branches: Vec<[u8; 32]>,
    pub version: u32,
    pub nbits: u32,
    pub ntime: u32,
    pub target: [u8; 32],
}

/// Result of mining computation
#[derive(Debug, Clone)]
pub struct MiningResult {
    pub nonce: u32,
    pub hash: [u8; 32],
    pub meets_target: bool,
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
    ) -> Result<Vec<MiningResult>>;
    
    /// Get the hasher's performance in hashes per second
    fn hashrate(&self) -> f64;
}
```

2. **Pool Client Trait** (`bminer-pool/src/lib.rs`)
```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub worker_name: String,
}

#[derive(Debug, Clone)]
pub struct Share {
    pub job_id: String,
    pub nonce: u32,
    pub ntime: u32,
    pub extranonce2: Vec<u8>,
}

#[async_trait]
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
```

3. **Monitor Trait** (`bminer-monitor/src/lib.rs`)
```rust
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct GpuMetrics {
    pub device_id: usize,
    pub temperature: u32,
    pub power_usage: u32,
    pub utilization: u32,
    pub memory_used: u64,
    pub fan_speed: u32,
}

#[async_trait::async_trait]
pub trait SystemMonitor: Send + Sync {
    /// Check if system is idle
    async fn is_idle(&self) -> Result<bool>;
    
    /// Get current system metrics
    async fn system_metrics(&self) -> Result<SystemMetrics>;
    
    /// Get GPU metrics for all devices
    async fn gpu_metrics(&self) -> Result<Vec<GpuMetrics>>;
}
```

**Validation**:
- All traits compile
- Documentation is clear
- Error types are comprehensive

---

### Task 3: CUDA Integration Layer

**Objective**: Implement Rust bindings for CUDA and GPU detection.

**Implementation** (`bminer-cuda/src/device.rs`):

```rust
use cudarc::driver::CudaDevice;
use nvml_wrapper::Nvml;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GpuError {
    #[error("CUDA error: {0}")]
    CudaError(String),
    #[error("NVML error: {0}")]
    NvmlError(String),
    #[error("No compatible GPUs found")]
    NoGpusFound,
}

pub type Result<T> = std::result::Result<T, GpuError>;

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub device_id: usize,
    pub name: String,
    pub compute_capability: (u32, u32),
    pub total_memory: u64,
    pub pci_bus_id: String,
}

pub struct GpuManager {
    nvml: Nvml,
    devices: Vec<CudaDevice>,
}

impl GpuManager {
    /// Initialize GPU manager and detect all NVIDIA GPUs
    pub fn new() -> Result<Self> {
        let nvml = Nvml::init()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        let device_count = nvml.device_count()
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        if device_count == 0 {
            return Err(GpuError::NoGpusFound);
        }
        
        let mut devices = Vec::new();
        for i in 0..device_count {
            match CudaDevice::new(i as usize) {
                Ok(device) => devices.push(device),
                Err(e) => eprintln!("Failed to initialize GPU {}: {}", i, e),
            }
        }
        
        if devices.is_empty() {
            return Err(GpuError::NoGpusFound);
        }
        
        Ok(Self { nvml, devices })
    }
    
    /// Get information about all available GPUs
    pub fn list_gpus(&self) -> Result<Vec<GpuInfo>> {
        let mut gpu_infos = Vec::new();
        
        for (idx, device) in self.devices.iter().enumerate() {
            let nvml_device = self.nvml.device_by_index(idx as u32)
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let name = nvml_device.name()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let memory_info = nvml_device.memory_info()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            let pci_info = nvml_device.pci_info()
                .map_err(|e| GpuError::NvmlError(e.to_string()))?;
            
            gpu_infos.push(GpuInfo {
                device_id: idx,
                name,
                compute_capability: device.compute_cap(),
                total_memory: memory_info.total,
                pci_bus_id: format!("{:04x}:{:02x}:{:02x}.0", 
                    pci_info.domain, pci_info.bus, pci_info.device),
            });
        }
        
        Ok(gpu_infos)
    }
    
    /// Get a specific GPU device
    pub fn get_device(&self, device_id: usize) -> Option<&CudaDevice> {
        self.devices.get(device_id)
    }
    
    /// Get GPU temperature
    pub fn get_temperature(&self, device_id: usize) -> Result<u32> {
        let device = self.nvml.device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }
    
    /// Get GPU power usage in milliwatts
    pub fn get_power_usage(&self, device_id: usize) -> Result<u32> {
        let device = self.nvml.device_by_index(device_id as u32)
            .map_err(|e| GpuError::NvmlError(e.to_string()))?;
        
        device.power_usage()
            .map_err(|e| GpuError::NvmlError(e.to_string()))
    }
}
```

**Validation**:
- GPU detection works on test system
- NVML queries return valid data
- Error handling is robust

---

### Task 4: Bitcoin Mining Algorithm

**Objective**: Implement SHA-256d hashing for Bitcoin proof-of-work.

**Implementation** (`bminer-core/src/hasher.rs`):

```rust
use sha2::{Sha256, Digest};

/// Bitcoin block header structure (80 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BlockHeader {
    pub version: u32,
    pub prev_block: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
}

impl BlockHeader {
    /// Serialize block header to bytes
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
        let first_hash = Sha256::digest(&bytes);
        let second_hash = Sha256::digest(&first_hash);
        second_hash.into()
    }
    
    /// Check if hash meets target difficulty
    pub fn meets_target(&self, target: &[u8; 32]) -> bool {
        let hash = self.hash();
        
        // Compare hash with target (both in little-endian)
        for i in (0..32).rev() {
            if hash[i] < target[i] {
                return true;
            } else if hash[i] > target[i] {
                return false;
            }
        }
        
        true // Equal to target
    }
}

/// Convert compact bits format to full 256-bit target
pub fn bits_to_target(bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    
    let exponent = (bits >> 24) as usize;
    let mantissa = bits & 0x00ffffff;
    
    if exponent <= 3 {
        let mantissa_bytes = mantissa.to_le_bytes();
        target[0..exponent].copy_from_slice(&mantissa_bytes[0..exponent]);
    } else {
        let mantissa_bytes = mantissa.to_le_bytes();
        let offset = exponent - 3;
        if offset < 29 {
            target[offset..offset+3].copy_from_slice(&mantissa_bytes[0..3]);
        }
    }
    
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sha256d() {
        // Genesis block header
        let header = BlockHeader {
            version: 1,
            prev_block: [0u8; 32],
            merkle_root: hex::decode(
                "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
            ).unwrap().try_into().unwrap(),
            timestamp: 1231006505,
            bits: 0x1d00ffff,
            nonce: 2083236893,
        };
        
        let hash = header.hash();
        let expected = hex::decode(
            "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
        ).unwrap();
        
        assert_eq!(&hash[..], &expected[..]);
    }
    
    #[test]
    fn test_bits_to_target() {
        let bits = 0x1d00ffff;
        let target = bits_to_target(bits);
        
        // Should produce a target with leading zeros
        assert_eq!(target[29], 0xff);
        assert_eq!(target[30], 0xff);
        assert_eq!(target[31], 0x00);
    }
}
```

**Validation**:
- Genesis block hash matches expected value
- Bits to target conversion is correct
- Target comparison works properly

---

## Phase 2: Mining Core (Week 3-4)

### Task 5: GPU Kernel Implementation

**Objective**: Write optimized CUDA kernels for parallel hash computation.

**CUDA Kernel** (`bminer-cuda/src/kernels/sha256.cu`):

```cuda
#include <stdint.h>

// SHA-256 constants
__constant__ uint32_t K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    // ... (full 64 constants)
};

// SHA-256 functions
#define ROTR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
#define CH(x, y, z) (((x) & (y)) ^ (~(x) & (z)))
#define MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))
#define EP0(x) (ROTR(x, 2) ^ ROTR(x, 13) ^ ROTR(x, 22))
#define EP1(x) (ROTR(x, 6) ^ ROTR(x, 11) ^ ROTR(x, 25))
#define SIG0(x) (ROTR(x, 7) ^ ROTR(x, 18) ^ ((x) >> 3))
#define SIG1(x) (ROTR(x, 17) ^ ROTR(x, 19) ^ ((x) >> 10))

__device__ void sha256_transform(uint32_t state[8], const uint32_t block[16]) {
    uint32_t a, b, c, d, e, f, g, h, t1, t2, m[64];
    
    // Prepare message schedule
    for (int i = 0; i < 16; i++) {
        m[i] = block[i];
    }
    for (int i = 16; i < 64; i++) {
        m[i] = SIG1(m[i-2]) + m[i-7] + SIG0(m[i-15]) + m[i-16];
    }
    
    // Initialize working variables
    a = state[0]; b = state[1]; c = state[2]; d = state[3];
    e = state[4]; f = state[5]; g = state[6]; h = state[7];
    
    // Main loop
    #pragma unroll
    for (int i = 0; i < 64; i++) {
        t1 = h + EP1(e) + CH(e, f, g) + K[i] + m[i];
        t2 = EP0(a) + MAJ(a, b, c);
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    
    // Add compressed chunk to current hash value
    state[0] += a; state[1] += b; state[2] += c; state[3] += d;
    state[4] += e; state[5] += f; state[6] += g; state[7] += h;
}

__global__ void bitcoin_mine(
    const uint32_t* header,      // 20 uint32_t (80 bytes)
    uint32_t start_nonce,
    uint32_t nonce_count,
    const uint32_t* target,      // 8 uint32_t (32 bytes)
    uint32_t* results,           // Output buffer
    uint32_t* result_count
) {
    uint32_t idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= nonce_count) return;
    
    uint32_t nonce = start_nonce + idx;
    
    // Copy header to local memory and set nonce
    uint32_t block[16];
    for (int i = 0; i < 19; i++) {
        block[i] = header[i];
    }
    block[19] = nonce;
    
    // First SHA-256
    uint32_t state1[8] = {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
    };
    sha256_transform(state1, block);
    
    // Padding for second block
    uint32_t block2[16] = {0};
    for (int i = 0; i < 8; i++) {
        block2[i] = state1[i];
    }
    block2[8] = 0x80000000;
    block2[15] = 256; // Length in bits
    
    sha256_transform(state1, block2);
    
    // Second SHA-256 (double hash)
    uint32_t state2[8] = {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
    };
    
    uint32_t final_block[16] = {0};
    for (int i = 0; i < 8; i++) {
        final_block[i] = state1[i];
    }
    final_block[8] = 0x80000000;
    final_block[15] = 256;
    
    sha256_transform(state2, final_block);
    
    // Check if hash meets target
    bool meets_target = true;
    for (int i = 7; i >= 0; i--) {
        if (state2[i] > target[i]) {
            meets_target = false;
            break;
        } else if (state2[i] < target[i]) {
            break;
        }
    }
    
    if (meets_target) {
        uint32_t pos = atomicAdd(result_count, 1);
        if (pos < 256) { // Max 256 results
            results[pos * 9] = nonce;
            for (int i = 0; i < 8; i++) {
                results[pos * 9 + i + 1] = state2[i];
            }
        }
    }
}
```

**Rust Wrapper** (`bminer-cuda/src/kernel.rs`):

```rust
use cudarc::driver::*;
use std::sync::Arc;

pub struct MiningKernel {
    device: Arc<CudaDevice>,
    module: CudaModule,
    function: CudaFunction,
}

impl MiningKernel {
    pub fn new(device: Arc<CudaDevice>) -> Result<Self> {
        // Load PTX or compile CUDA source
        let ptx = compile_ptx("src/kernels/sha256.cu")?;
        let module = device.load_ptx(ptx, "bitcoin_mine", &[])?;
        let function = module.get_function("bitcoin_mine")?;
        
        Ok(Self {
            device,
            module,
            function,
        })
    }
    
    pub async fn mine(
        &self,
        header: &[u32; 20],
        start_nonce: u32,
        nonce_count: u32,
        target: &[u32; 8],
    ) -> Result<Vec<(u32, [u32; 8])>> {
        // Allocate device memory
        let header_dev = self.device.htod_copy(header.as_slice())?;
        let target_dev = self.device.htod_copy(target.as_slice())?;
        let results_dev = self.device.alloc_zeros::<u32>(256 * 9)?;
        let count_dev = self.device.alloc_zeros::<u32>(1)?;
        
        // Launch kernel
        let threads_per_block = 256;
        let blocks = (nonce_count + threads_per_block - 1) / threads_per_block;
        
        let cfg = LaunchConfig {
            grid_dim: (blocks, 1, 1),
            block_dim: (threads_per_block, 1, 1),
            shared_mem_bytes: 0,
        };
        
        unsafe {
            self.function.launch(cfg, (
                &header_dev,
                start_nonce,
                nonce_count,
                &target_dev,
                &results_dev,
                &count_dev,
            ))?;
        }
        
        // Copy results back
        let count = self.device.dtoh_sync_copy(&count_dev)?;
        let results = self.device.dtoh_sync_copy(&results_dev)?;
        
        // Parse results
        let mut found = Vec::new();
        for i in 0..count[0].min(256) as usize {
            let nonce = results[i * 9];
            let mut hash = [0u32; 8];
            hash.copy_from_slice(&results[i * 9 + 1..i * 9 + 9]);
            found.push((nonce, hash));
        }
        
        Ok(found)
    }
}
```

**Validation**:
- Kernel compiles without errors
- Hash computation matches CPU implementation
- Performance meets targets (100+ MH/s)

---

## Phase 3-5: Remaining Implementation

Due to length constraints, the remaining phases follow similar patterns:

### Phase 3: Management
- Implement Stratum protocol client
- Build idle detection using sysinfo
- Create TOML configuration parser
- Set up tracing-based logging

### Phase 4: Reliability
- Implement work scheduler with tokio
- Add error recovery with exponential backoff
- Create connection pool management

### Phase 5: Quality
- Write comprehensive test suite
- Generate documentation with rustdoc
- Create GitHub Actions CI/CD pipeline

## Development Best Practices

1. **Test-Driven Development**: Write tests before implementation
2. **Incremental Development**: Build and test each component independently
3. **Performance Profiling**: Use criterion for benchmarks
4. **Code Review**: Review all changes before merging
5. **Documentation**: Document all public APIs

## Success Criteria

- [ ] All unit tests pass
- [ ] Integration tests demonstrate end-to-end mining
- [ ] Performance benchmarks meet targets
- [ ] Documentation is complete and accurate
- [ ] Code passes clippy lints
- [ ] Binary runs successfully on target systems