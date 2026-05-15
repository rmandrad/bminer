//! BMiner CLI - Command-line interface for the Bitcoin miner

mod config;
mod miner;

use anyhow::anyhow;
use bminer_core::{Hasher, Work};
use bminer_cuda::device::list_cuda_devices;
use bminer_cuda::hasher::CudaHasher;
use clap::Parser;
use config::BminerConfig;
use miner::Miner;
use std::path::Path;
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
    configure_wsl_library_paths();
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

                apply_cli_overrides(&mut config, &args)?;

                if let Some(intensity) = args.intensity {
                    config.gpu.intensity = intensity;
                    config.validate()?;
                    tracing::info!("Overriding mining intensity from CLI: {}", intensity);
                }

                let resolved_devices = resolve_configured_cuda_devices(&config.gpu.devices)?;
                config.gpu.devices = resolved_devices;

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
        tracing::info!(
            "Dry run mode - configuration would be loaded from: {}",
            args.config
        );
    }

    Ok(())
}

fn apply_cli_overrides(config: &mut BminerConfig, args: &Args) -> anyhow::Result<()> {
    if let Some(devices) = &args.devices {
        config.gpu.devices = parse_device_list(devices)?;
        config.validate()?;
        tracing::info!("Overriding GPU devices from CLI: {:?}", config.gpu.devices);
    }

    Ok(())
}

fn parse_device_list(devices: &str) -> anyhow::Result<Vec<usize>> {
    let parsed = devices
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<usize>()
                .map_err(|error| anyhow!("Invalid GPU device ID `{entry}`: {error}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if parsed.is_empty() {
        anyhow::bail!("GPU device list cannot be empty. Use comma-separated zero-based IDs such as `0` or `0,1`.");
    }

    Ok(parsed)
}

fn resolve_configured_cuda_devices(configured_devices: &[usize]) -> anyhow::Result<Vec<usize>> {
    let available_devices = list_cuda_devices()
        .map_err(|error| {
            anyhow!(
                "Unable to enumerate CUDA devices: {}. If you're running under WSL2, confirm the NVIDIA driver is available inside WSL, `nvidia-smi` works, and `/usr/lib/wsl/lib/libcuda.so` exists.",
                error
            )
        })?
        .into_iter()
        .map(|device| device.device_id)
        .collect::<Vec<_>>();

    resolve_requested_devices(configured_devices, &available_devices)
}

fn resolve_requested_devices(
    configured_devices: &[usize],
    available_devices: &[usize],
) -> anyhow::Result<Vec<usize>> {
    if available_devices.is_empty() {
        anyhow::bail!(
            "No CUDA devices are available. `--gpu-info` should list at least one device before mining or benchmarking can run."
        );
    }

    if configured_devices.is_empty() {
        return Ok(available_devices.to_vec());
    }

    let missing_devices = configured_devices
        .iter()
        .copied()
        .filter(|device_id| !available_devices.contains(device_id))
        .collect::<Vec<_>>();

    if !missing_devices.is_empty() {
        anyhow::bail!(
            "Configured GPU device ID(s) {:?} are unavailable. Available CUDA device IDs: {:?}. Device IDs are zero-based, so the first GPU is `0`.",
            missing_devices,
            available_devices
        );
    }

    Ok(configured_devices.to_vec())
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

fn configure_wsl_library_paths() {
    let mut candidates = vec![
        Path::new("/usr/lib/wsl/lib"),
        Path::new("/opt/cuda/lib64"),
        Path::new("/opt/cuda/targets/x86_64-linux/lib"),
        Path::new("/usr/local/cuda/lib64"),
        Path::new("/usr/local/cuda-12/lib64"),
        Path::new("/usr/local/cuda-12.5/lib64"),
    ];

    let custom_cuda_lib_dir = std::env::var_os("BMINER_CUDA_LIB_DIR");
    if let Some(path) = custom_cuda_lib_dir.as_deref().map(Path::new) {
        candidates.push(path);
    }

    for candidate in candidates {
        append_library_path(candidate);
    }
}

fn append_library_path(path: &Path) {
    if !path.exists() {
        return;
    }

    let current = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
    let current_string = current.to_string_lossy();

    if current_string
        .split(':')
        .any(|entry| entry == path.to_string_lossy())
    {
        return;
    }

    let updated = if current_string.is_empty() {
        path.to_string_lossy().into_owned()
    } else {
        format!("{}:{}", path.display(), current_string)
    };

    unsafe {
        std::env::set_var("LD_LIBRARY_PATH", updated);
    }
}

async fn show_gpu_info() -> anyhow::Result<()> {
    use bminer_cuda::device::{list_cuda_devices, GpuManager};

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
    apply_cli_overrides(&mut config, args)?;

    if let Some(intensity) = args.intensity {
        config.gpu.intensity = intensity;
        config.validate()?;
    }

    let resolved_devices = resolve_configured_cuda_devices(&config.gpu.devices)?;
    let device_id = resolved_devices[0];
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
            let hasher =
                CudaHasher::new_with_launch_params(device_id, threads_per_block, blocks_per_grid)?;

            if !hasher.is_cuda_backend() {
                println!(
                    "threads/block={:<4} blocks/grid={:<4} => CUDA unavailable ({})",
                    threads_per_block,
                    blocks_per_grid,
                    hasher.device_name()
                );
                continue;
            }

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
                threads_per_block, blocks_per_grid, mh_s
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
            threads_per_block, blocks_per_grid, mh_s
        );
        println!("\nSuggested config:");
        println!("threads_per_block = {}", threads_per_block);
        println!("blocks_per_grid = {}", blocks_per_grid);
    } else {
        anyhow::bail!(
            "CUDA was unavailable for every benchmark configuration. On WSL, try running via ./bminer.sh or confirm /usr/lib/wsl/lib/libcuda.so is accessible."
        );
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

#[cfg(test)]
mod tests {
    use super::{parse_device_list, resolve_requested_devices};

    #[test]
    fn parse_device_list_accepts_zero_based_csv() {
        assert_eq!(parse_device_list("0, 1,2").unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn parse_device_list_rejects_empty_input() {
        let error = parse_device_list(" , ").unwrap_err().to_string();
        assert!(error.contains("cannot be empty"));
    }

    #[test]
    fn resolve_requested_devices_defaults_to_all_available_devices() {
        let resolved = resolve_requested_devices(&[], &[0, 2]).unwrap();
        assert_eq!(resolved, vec![0, 2]);
    }

    #[test]
    fn resolve_requested_devices_reports_invalid_device_ids() {
        let error = resolve_requested_devices(&[1], &[0])
            .unwrap_err()
            .to_string();
        assert!(error.contains("zero-based"));
        assert!(error.contains("[1]"));
        assert!(error.contains("[0]"));
    }
}

// Made with Bob
