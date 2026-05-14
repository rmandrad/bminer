//! Mining coordinator and work scheduler
//!
//! This module orchestrates the mining process, coordinating between
//! the pool client, GPU hashers, and idle detection.

use bminer_core::{Hasher, Work};
use bminer_cuda::device::GpuManager;
use bminer_cuda::hasher::CudaHasher;
use bminer_monitor::idle::IdleDetector;
use bminer_pool::{PoolClient, Share};
use bminer_pool::stratum::StratumClient;
use crate::config::BminerConfig;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Mining statistics
#[derive(Debug, Clone, Default)]
pub struct MiningStats {
    /// Total hashes computed
    pub total_hashes: u64,
    
    /// Shares submitted
    pub shares_submitted: u64,
    
    /// Shares accepted
    pub shares_accepted: u64,
    
    /// Shares rejected
    pub shares_rejected: u64,
    
    /// Mining start time
    pub start_time: Option<Instant>,
    
    /// Last share time
    pub last_share: Option<Instant>,
}

enum MineWorkOutcome {
    Completed,
    GpuPaused,
}

impl MiningStats {
    /// Calculate current hashrate
    pub fn hashrate(&self) -> f64 {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return self.total_hashes as f64 / elapsed;
            }
        }
        0.0
    }
    
    /// Calculate acceptance rate
    pub fn acceptance_rate(&self) -> f64 {
        if self.shares_submitted > 0 {
            (self.shares_accepted as f64 / self.shares_submitted as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// Mining coordinator
pub struct Miner {
    config: BminerConfig,
    pool_client: Arc<Mutex<StratumClient>>,
    hashers: Vec<Arc<CudaHasher>>,
    idle_detector: Arc<Mutex<IdleDetector>>,
    stats: Arc<Mutex<MiningStats>>,
    running: Arc<Mutex<bool>>,
}

impl Miner {
    /// Create a new miner
    pub fn new(config: BminerConfig) -> anyhow::Result<Self> {
        // Create pool client
        let pool_client = StratumClient::new(config.pool.clone());
        
        // Create hashers for each GPU
        let mut hashers = Vec::new();
        let devices = if config.gpu.devices.is_empty() {
            // Use all available GPUs
            vec![0] // For now, just use GPU 0
        } else {
            config.gpu.devices.clone()
        };
        
        for device_id in devices {
            match CudaHasher::new_with_launch_params(
                device_id,
                config.gpu.threads_per_block,
                config.gpu.blocks_per_grid,
            ) {
                Ok(hasher) => {
                    tracing::info!(
                        "Initialized hasher for device {}: {} with launch params: {} threads/block, {} blocks/grid",
                        device_id,
                        hasher.device_name(),
                        config.gpu.threads_per_block,
                        config.gpu.blocks_per_grid
                    );
                    hashers.push(Arc::new(hasher));
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize GPU {}: {}", device_id, e);
                }
            }
        }
        
        if hashers.is_empty() {
            anyhow::bail!("No GPUs available for mining");
        }
        
        // Create idle detector
        let idle_config = bminer_monitor::idle::IdleConfig {
            cpu_threshold: config.idle.cpu_threshold,
            min_idle_duration: config.idle.min_idle_duration,
            check_interval: config.idle.check_interval,
            gpu_threshold: config.idle.gpu_threshold,
        };
        let idle_detector = IdleDetector::new(idle_config);
        
        Ok(Self {
            config,
            pool_client: Arc::new(Mutex::new(pool_client)),
            hashers,
            idle_detector: Arc::new(Mutex::new(idle_detector)),
            stats: Arc::new(Mutex::new(MiningStats::default())),
            running: Arc::new(Mutex::new(false)),
        })
    }
    
    /// Start mining
    pub async fn start(&self) -> anyhow::Result<()> {
        tracing::info!("Starting BMiner...");
        let (nonce_range, pause_duration) = self.intensity_settings();
        tracing::info!(
            "Mining intensity set to {} ({} nonces/chunk, {} ms pause between chunks)",
            self.config.gpu.intensity,
            nonce_range,
            pause_duration.as_millis()
        );

        if self.config.monitoring.enabled {
            tracing::info!(
                "Performance monitoring enabled (stats interval: {}s)",
                self.config.monitoring.stats_interval
            );
        } else {
            tracing::info!("Performance monitoring disabled");
        }

        if self.config.monitoring.web_dashboard {
            tracing::warn!("Web dashboard is enabled in config but is not implemented yet");
        }
        
        // Set running flag
        *self.running.lock().await = true;
        
        // Connect to pool
        {
            let mut client = self.pool_client.lock().await;
            client.connect().await?;
            client.subscribe().await?;
            client.authorize().await?;
        }
        
        tracing::info!("Connected to pool successfully");
        
        // Start mining loop
        self.mining_loop().await?;
        
        Ok(())
    }
    
    /// Stop mining
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping BMiner...");
        *self.running.lock().await = false;
        
        // Disconnect from pool
        let mut client = self.pool_client.lock().await;
        client.disconnect().await?;
        
        Ok(())
    }
    
    /// Main mining loop
    async fn mining_loop(&self) -> anyhow::Result<()> {
        let mut stats = self.stats.lock().await;
        stats.start_time = Some(Instant::now());
        drop(stats);
        let mut gpu_paused = false;
        
        loop {
            // Check if we should stop
            if !*self.running.lock().await {
                break;
            }
            
            // Check if system is idle (if enabled)
            if self.config.idle.enabled {
                let mut detector = self.idle_detector.lock().await;
                if !detector.is_idle()? {
                    tracing::debug!("System not idle, waiting...");
                    drop(detector);
                    sleep(Duration::from_secs(self.config.idle.check_interval)).await;
                    continue;
                }
                drop(detector);
            }

            if !self.gpu_within_limits()? {
                gpu_paused = true;
                sleep(Duration::from_secs(self.config.idle.check_interval.max(1))).await;
                continue;
            }

            if gpu_paused {
                tracing::info!("GPU back within configured limits, resuming mining");
                gpu_paused = false;
            }
            
            // Get work from pool
            let work = {
                let mut client = self.pool_client.lock().await;
                match client.receive_work().await {
                    Ok(work) => work,
                    Err(e) => {
                        tracing::error!("Failed to receive work: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
            };
            
            tracing::info!("Received new work: job_id={}", work.job_id);
            
            // Mine the work
            match self.mine_work(work).await {
                Ok(MineWorkOutcome::Completed) => {}
                Ok(MineWorkOutcome::GpuPaused) => {
                    gpu_paused = true;
                }
                Err(e) => {
                    tracing::error!("Mining error: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Mine a work unit
    async fn mine_work(&self, work: Work) -> anyhow::Result<MineWorkOutcome> {
        let (nonce_range, pause_duration) = self.intensity_settings();
        let mut start_nonce = 0u32;
        let mut last_stats_log = Instant::now();
        
        // Use first hasher for now (multi-GPU support can be added later)
        let hasher = &self.hashers[0];
        
        loop {
            // Check if we should stop
            if !*self.running.lock().await {
                break;
            }
            
            // Check if system is still idle
            if self.config.idle.enabled {
                let mut detector = self.idle_detector.lock().await;
                if !detector.is_idle()? {
                    tracing::info!("System no longer idle, pausing mining");
                    drop(detector);
                    break;
                }
                drop(detector);
            }

            if !self.gpu_within_limits()? {
                tracing::warn!("GPU limits exceeded, pausing mining until the device cools down");
                sleep(Duration::from_secs(self.config.idle.check_interval.max(1))).await;
                return Ok(MineWorkOutcome::GpuPaused);
            }
            
            // Mine a range of nonces
            let extranonce2 = vec![0u8; work.extranonce2_size];
            let results = hasher.hash_range(&work, start_nonce, nonce_range, &extranonce2).await?;
            
            // Update stats
            {
                let mut stats = self.stats.lock().await;
                stats.total_hashes += nonce_range as u64;
            }
            
            // Check for valid shares
            for result in results {
                if result.meets_target {
                    tracing::info!("Found valid share! nonce={}", result.nonce);
                    
                    // Submit share
                    let share = Share {
                        job_id: work.job_id.clone(),
                        nonce: result.nonce,
                        ntime: work.ntime,
                        extranonce2: result.extranonce2.clone(),
                    };
                    
                    let mut client = self.pool_client.lock().await;
                    match client.submit_share(share).await {
                        Ok(accepted) => {
                            let mut stats = self.stats.lock().await;
                            stats.shares_submitted += 1;
                            stats.last_share = Some(Instant::now());
                            
                            if accepted {
                                stats.shares_accepted += 1;
                                tracing::info!("Share accepted! ({}/{})", 
                                              stats.shares_accepted, stats.shares_submitted);
                            } else {
                                stats.shares_rejected += 1;
                                tracing::warn!("Share rejected ({}/{})", 
                                              stats.shares_rejected, stats.shares_submitted);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to submit share: {}", e);
                        }
                    }
                }
            }
            
            // Move to next nonce range
            start_nonce = start_nonce.wrapping_add(nonce_range);

            if !pause_duration.is_zero() {
                sleep(pause_duration).await;
            }
            
            // Log progress periodically
            if self.config.monitoring.enabled
                && last_stats_log.elapsed()
                    >= Duration::from_secs(self.config.monitoring.stats_interval)
            {
                let stats = self.stats.lock().await;
                tracing::info!("Mining: {:.2} MH/s, {} shares ({:.1}% accepted)", 
                              stats.hashrate() / 1_000_000.0,
                              stats.shares_submitted,
                              stats.acceptance_rate());
                last_stats_log = Instant::now();
            }
        }
        
        Ok(MineWorkOutcome::Completed)
    }
    
    /// Get current mining statistics
    pub async fn get_stats(&self) -> MiningStats {
        self.stats.lock().await.clone()
    }

    fn configured_device_ids(&self) -> Vec<usize> {
        if self.config.gpu.devices.is_empty() {
            vec![0]
        } else {
            self.config.gpu.devices.clone()
        }
    }

    fn gpu_within_limits(&self) -> anyhow::Result<bool> {
        let manager = match GpuManager::new() {
            Ok(manager) => manager,
            Err(err) => {
                tracing::warn!("Unable to read GPU telemetry, skipping thermal checks: {}", err);
                return Ok(true);
            }
        };

        for device_id in self.configured_device_ids() {
            match manager.get_temperature(device_id) {
                Ok(temperature) => {
                    if temperature >= self.config.gpu.max_temperature {
                        tracing::warn!(
                            "GPU {} temperature is {}C, above configured max {}C",
                            device_id,
                            temperature,
                            self.config.gpu.max_temperature
                        );
                        return Ok(false);
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        "Skipping temperature check for GPU {} because telemetry is unavailable: {}",
                        device_id,
                        err
                    );
                }
            }

            if self.config.gpu.max_power > 0 {
                match manager.get_power_usage(device_id) {
                    Ok(power_usage) => {
                        let power_watts = power_usage / 1000;

                        if power_watts >= self.config.gpu.max_power {
                            tracing::warn!(
                                "GPU {} power usage is {}W, above configured max {}W",
                                device_id,
                                power_watts,
                                self.config.gpu.max_power
                            );
                            return Ok(false);
                        }
                    }
                    Err(err) => {
                        tracing::debug!(
                            "Skipping power check for GPU {} because telemetry is unavailable: {}",
                            device_id,
                            err
                        );
                    }
                }
            }
        }

        Ok(true)
    }

    fn intensity_settings(&self) -> (u32, Duration) {
        let intensity = self.config.gpu.intensity.clamp(1, 10) as u32;
        let nonce_range = 1_000_000u32.saturating_mul(intensity);
        let pause_ms = (10 - intensity) * 2;
        (nonce_range, Duration::from_millis(pause_ms as u64))
    }
}

// Made with Bob
