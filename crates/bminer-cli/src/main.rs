//! BMiner CLI - Command-line interface for the Bitcoin miner

mod config;
mod miner;

use clap::Parser;
use config::BminerConfig;
use miner::Miner;

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
    
    // Load configuration if not in info mode
    if !args.dry_run && !args.benchmark {
        match BminerConfig::from_file(&args.config) {
            Ok(config) => {
                tracing::info!("Configuration loaded successfully");
                
                // Create and start miner
                let miner = Miner::new(config)?;
                
                // Set up Ctrl+C handler
                let miner_clone = std::sync::Arc::new(miner);
                let miner_for_ctrlc = miner_clone.clone();
                
                ctrlc::set_handler(move || {
                    tracing::info!("Received Ctrl+C, shutting down...");
                    let miner = miner_for_ctrlc.clone();
                    tokio::spawn(async move {
                        let _ = miner.stop().await;
                        std::process::exit(0);
                    });
                })?;
                
                // Start mining
                miner_clone.start().await?;
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
    use bminer_cuda::device::GpuManager;
    
    tracing::info!("Detecting GPUs...");
    
    match GpuManager::new() {
        Ok(manager) => {
            let gpus = manager.list_gpus()?;
            
            println!("\n=== GPU Information ===\n");
            
            for gpu in gpus {
                println!("Device {}: {}", gpu.device_id, gpu.name);
                println!("  Memory: {} GB", gpu.total_memory / 1024 / 1024 / 1024);
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
            tracing::error!("Failed to initialize GPU manager: {}", e);
            println!("\nNo compatible NVIDIA GPUs found.");
            println!("Please ensure:");
            println!("  - NVIDIA drivers are installed");
            println!("  - CUDA toolkit is installed");
            println!("  - You have at least one NVIDIA GPU");
        }
    }
    
    Ok(())
}

// Made with Bob
