//! BMiner CLI - Command-line interface for the Bitcoin miner

mod config;
mod miner;

use clap::Parser;
use bminer_core::{Hasher, Work};
use bminer_cuda::hasher::CudaHasher;
use config::BminerConfig;
use miner::Miner;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// BMiner - Bitcoin GPU Miner
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "~/.config/bminer/bminer.toml")]
    config: String,
    
    /// GPU devices to use (comma-separated, e.g., "0,1")
    #[arg(short, long)]
    devices: Option<String>,
    
    /// Mining intensity (1-10)
    #[arg(short, long)]
    intensity: Option<u8>,
    
    /// Show GPU information and exit
    #[arg(long)]
    gpu_info: bool,
    
    /// Show statistics
    #[arg(long)]
    stats: bool,
    
    /// Run in benchmark mode
    #[arg(long)]
    benchmark: bool,
    
    /// Dry run (test configuration without mining)
    #[arg(long)]
    dry_run: bool,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(parse_log_level(&args.log_level))
        .init();
    
    tracing::info!("BMiner v{} starting...", env!("CARGO_PKG_VERSION"));
    
    // Show GPU info if requested
    if args.gpu_info {
        show_gpu_info().await?;
        return Ok(());
    }

    if args.benchmark {
        run_benchmark(&args).await?;
        return Ok(());
    }
    
    // Load configuration if not in info mode
    if !args.dry_run {
        match BminerConfig::from_file(&args.config) {
            Ok(config) => {
                tracing::info!("Configuration loaded successfully");
                let mut config = config;

                if let Some(intensity) = args.intensity {
                    config.gpu.intensity = intensity;
                    config.validate()?;
                    tracing::info!("Overriding mining intensity from CLI: {}", intensity);
                }

                // Create and start miner
                let miner = Arc::new(Miner::new(config)?);
                let mut miner_task = {
                    let miner = miner.clone();
                    tokio::spawn(async move { miner.start().await })
                };

                tokio::select! {
                    result = &mut miner_task => {
                        result??;
                    }
                    signal = tokio::signal::ctrl_c() => {
                        signal?;
                        tracing::info!("Received Ctrl+C, shutting down...");

                        miner_task.abort();
                        let _ = miner_task.await;

                        miner.stop().await?;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to load configuration: {}", e);
                tracing::info!("Please create a configuration file at: {}", args.config);
                tracing::info!("See config/bminer.toml.example for reference");
                return Err(e.into());
            }
        }
    } else if args.dry_run {
        tracing::info!("Dry run mode - configuration would be loaded from: {}", args.config);
    }
    
    Ok(())
}

fn parse_log_level(level: &str) -> tracing::Level {
    match level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    }
}

async fn show_gpu_info() -> anyhow::Result<()> {
    use bminer_cuda::device::{GpuManager, list_cuda_devices};
    
    tracing::info!("Detecting GPUs...");
    
    match GpuManager::new() {
        Ok(manager) => {
            let gpus = manager.list_gpus()?;
            
            println!("\n=== GPU Information ===\n");
            
            for gpu in gpus {
                println!("Device {}: {}", gpu.device_id, gpu.name);
                if gpu.total_memory > 0 {
                    println!("  Memory: {} GB", gpu.total_memory / 1024 / 1024 / 1024);
                } else {
                    println!("  Memory: unavailable");
                }
                println!("  PCI Bus: {}", gpu.pci_bus_id);
                
                if let Ok(temp) = manager.get_temperature(gpu.device_id) {
                    println!("  Temperature: {}°C", temp);
                }
                
                if let Ok(power) = manager.get_power_usage(gpu.device_id) {
                    println!("  Power: {} W", power / 1000);
                }
                
                if let Ok(util) = manager.get_utilization(gpu.device_id) {
                    println!("  Utilization: {}%", util);
                }
                
                println!();
            }
        }
        Err(e) => {
            tracing::warn!("Failed to initialize NVML telemetry: {}", e);

            match list_cuda_devices() {
                Ok(devices) => {
                    println!("\n=== CUDA Devices ===\n");

                    for device in devices {
                        println!("Device {}: {}", device.device_id, device.name);
                    }

                    println!();
                    println!("NVML telemetry is unavailable on this system, so temperature, power, and utilization are not shown.");
                    println!("Mining may still work if CUDA is functional.");
                }
                Err(cuda_error) => {
                    tracing::error!("Failed to enumerate CUDA devices: {}", cuda_error);
                    println!("\nNo compatible NVIDIA GPUs found.");
                    println!("Please ensure:");
                    println!("  - NVIDIA drivers are installed");
                    println!("  - CUDA toolkit is installed");
                    println!("  - You have at least one NVIDIA GPU");
                }
            }
        }
    }
    
    Ok(())
}

async fn run_benchmark(args: &Args) -> anyhow::Result<()> {
    tracing::info!("Benchmark mode");

    let mut config = BminerConfig::from_file(&args.config)?;

    if let Some(intensity) = args.intensity {
        config.gpu.intensity = intensity;
        config.validate()?;
    }

    let device_id = config.gpu.devices.first().copied().unwrap_or(0);
    let intensity = config.gpu.intensity.clamp(1, 10) as u32;
    let nonce_range = 1_000_000u32.saturating_mul(intensity);
    let mut work = benchmark_work();
    work.extranonce2_size = 4;
    let extranonce2 = vec![0u8; work.extranonce2_size];

    let thread_candidates = [128u32, 256, 512];
    let block_candidates = [512u32, 1024, 2048, 4096];
    let mut best: Option<(u32, u32, f64)> = None;

    println!("\n=== Benchmark ===\n");
    println!("Device: {}", device_id);
    println!("Nonce range per launch: {}", nonce_range);
    println!("Each configuration runs for ~2 seconds.\n");

    for threads_per_block in thread_candidates {
        for blocks_per_grid in block_candidates {
            let hasher = CudaHasher::new_with_launch_params(
                device_id,
                threads_per_block,
                blocks_per_grid,
            )?;

            let start = Instant::now();
            let deadline = start + Duration::from_secs(2);
            let mut start_nonce = 0u32;
            let mut total_hashes = 0u64;

            while Instant::now() < deadline {
                let _ = hasher
                    .hash_range(&work, start_nonce, nonce_range, &extranonce2)
                    .await?;
                total_hashes += nonce_range as u64;
                start_nonce = start_nonce.wrapping_add(nonce_range);
            }

            let elapsed = start.elapsed().as_secs_f64().max(f64::EPSILON);
            let mh_s = total_hashes as f64 / elapsed / 1_000_000.0;

            println!(
                "threads/block={:<4} blocks/grid={:<4} => {:>7.2} MH/s",
                threads_per_block,
                blocks_per_grid,
                mh_s
            );

            match best {
                Some((_, _, best_rate)) if mh_s <= best_rate => {}
                _ => best = Some((threads_per_block, blocks_per_grid, mh_s)),
            }
        }
    }

    if let Some((threads_per_block, blocks_per_grid, mh_s)) = best {
        println!("\nBest configuration:");
        println!(
            "threads_per_block = {}\nblocks_per_grid = {}\nobserved = {:.2} MH/s",
            threads_per_block,
            blocks_per_grid,
            mh_s
        );
        println!("\nSuggested config:");
        println!("threads_per_block = {}", threads_per_block);
        println!("blocks_per_grid = {}", blocks_per_grid);
    }

    Ok(())
}

fn benchmark_work() -> Work {
    let mut work = Work::new(
        "benchmark".to_string(),
        [0u8; 32],
        vec![0u8; 128],
        Vec::new(),
        1,
        0x1d00ffff,
        1234567890,
    );
    work.target = [0u8; 32];
    work.extranonce1 = vec![0u8; 8];
    work
}

// Made with Bob
