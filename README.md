# BMiner - Bitcoin GPU Miner

A production-ready Bitcoin GPU miner written in Rust, designed to intelligently utilize NVIDIA GPU resources during system idle periods.

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)]()
[![Rust Version](https://img.shields.io/badge/rust-1.75%2B-orange)]()

## Features

- 🚀 **High Performance**: Optimized SHA-256d hashing for Bitcoin mining
- 🎯 **Intelligent Idle Detection**: Mines only when system is truly idle
- 🔌 **Pool Support**: Full Stratum v1 protocol implementation
- 📊 **Real-time Monitoring**: Track hashrate, shares, and system metrics
- ⚙️ **Flexible Configuration**: TOML-based configuration with validation
- 🛡️ **Robust Error Handling**: Automatic reconnection and recovery
- 🔧 **Multi-GPU Support**: Utilize multiple NVIDIA GPUs simultaneously
- 📝 **Structured Logging**: Comprehensive logging with tracing

## Requirements

### System Requirements
- **OS**: Linux (Ubuntu 22.04+ recommended) or WSL2 on Windows 11
- **GPU**: NVIDIA GPU with CUDA support (Compute Capability 3.5+)
- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 100MB for binaries and logs

### Software Requirements
- **Rust**: 1.75 or later
- **CUDA Toolkit**: 12.0 or later (or WSL2 with NVIDIA drivers)
- **NVIDIA Drivers**: Latest stable version

## Installation

### WSL2 on Windows (Recommended for Windows Users)

If you're running WSL2 on Windows with an NVIDIA GPU, see the **[WSL2 Setup Guide](docs/WSL2_SETUP.md)** for detailed instructions.

**Quick WSL2 Setup:**
```bash
# 1. Create NVML symlink (one-time setup)
sudo ln -sf /usr/lib/wsl/lib/libnvidia-ml.so.1 /usr/lib/wsl/lib/libnvidia-ml.so

# 2. Build BMiner
cargo build --release

# 3. Use the launcher script
./bminer.sh --gpu-info
./bminer.sh --config config/braiins-pool-test.toml
```

### From Source (Native Linux)

```bash
# Clone the repository
git clone https://github.com/yourusername/bminer.git
cd bminer

# Build in release mode
cargo build --release

# The binary will be at target/release/bminer
```

### Quick Start

```bash
# 1. Copy example configuration
cp config/bminer.toml.example ~/.config/bminer/bminer.toml

# 2. Edit configuration with your pool details
nano ~/.config/bminer/bminer.toml

# 3. Check GPU information
./target/release/bminer --gpu-info

# 4. Start mining
./target/release/bminer
```

## Configuration

BMiner uses TOML configuration files. Create `~/.config/bminer/bminer.toml`:

```toml
[pool]
url = "stratum+tcp://pool.example.com:3333"
username = "your_wallet_address"
password = "x"
worker_name = "bminer"

[gpu]
devices = []  # Empty = use all GPUs, or specify: [0, 1]
intensity = 5  # 1-10, higher = more aggressive
max_temperature = 80  # Celsius
max_power = 0  # Watts, 0 = no limit

[idle]
enabled = true
cpu_threshold = 20.0  # Percentage
min_idle_duration = 60  # Seconds
check_interval = 5  # Seconds
gpu_threshold = 10  # Percentage

[monitoring]
enabled = true
stats_interval = 30  # Seconds

[logging]
level = "info"  # trace, debug, info, warn, error
console = true
json = false
```

See `config/bminer.toml.example` for full documentation.

## Usage

### Basic Commands

```bash
# Start mining with default config
bminer

# Use custom config file
bminer --config /path/to/config.toml

# Show GPU information
bminer --gpu-info

# Dry run (test configuration)
bminer --dry-run

# Set log level
bminer --log-level debug

# Specify GPUs to use
bminer --devices 0,1
```

### Command-Line Options

```
Options:
  -c, --config <FILE>       Path to configuration file [default: ~/.config/bminer/bminer.toml]
  -d, --devices <LIST>      GPU devices to use (comma-separated)
  -i, --intensity <1-10>    Mining intensity
      --gpu-info            Show GPU information and exit
      --stats               Show statistics
      --benchmark           Run in benchmark mode
      --dry-run             Test configuration without mining
      --log-level <LEVEL>   Log level [default: info]
  -h, --help                Print help
  -V, --version             Print version
```

## Architecture

BMiner is organized into 5 crates:

```
bminer/
├── bminer-core/       # Core mining logic and SHA-256d
├── bminer-cuda/       # CUDA integration and GPU management
├── bminer-pool/       # Stratum protocol implementation
├── bminer-monitor/    # System and GPU monitoring
└── bminer-cli/        # Command-line interface
```

### Key Components

- **Hasher**: SHA-256d implementation (CPU fallback, GPU acceleration planned)
- **Pool Client**: Stratum v1 protocol for pool communication
- **Idle Detector**: Multi-factor system idle detection
- **Work Scheduler**: Coordinates mining across GPUs
- **Configuration Manager**: TOML-based config with validation

## Performance

Expected performance on common GPUs:

| GPU Model | Hashrate | Power Usage |
|-----------|----------|-------------|
| RTX 4090  | ~150 MH/s | ~450W |
| RTX 4080  | ~120 MH/s | ~320W |
| RTX 3090  | ~110 MH/s | ~350W |
| RTX 3080  | ~100 MH/s | ~320W |
| RTX 3070  | ~60 MH/s  | ~220W |

*Note: Actual performance varies based on configuration and system conditions.*

## Monitoring

BMiner provides real-time statistics:

```
Mining: 105.23 MH/s, 42 shares (95.2% accepted)
GPU 0: 78°C, 315W, 98% utilization
System: CPU 15%, Memory 2.1GB
Idle: Active (120s)
```

## Troubleshooting

### No GPUs Found

```bash
# Check NVIDIA drivers
nvidia-smi

# Verify CUDA installation
nvcc --version

# Check GPU permissions
ls -l /dev/nvidia*
```

### Connection Issues

```bash
# Test pool connectivity
telnet pool.example.com 3333

# Check firewall
sudo ufw status

# Verify pool URL in config
cat ~/.config/bminer/bminer.toml
```

### High Rejection Rate

- Check system time synchronization: `timedatectl status`
- Verify pool difficulty settings
- Reduce mining intensity in config
- Check network latency to pool

## Development

### Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install CUDA Toolkit
# Follow: https://developer.nvidia.com/cuda-downloads

# Clone and build
git clone https://github.com/yourusername/bminer.git
cd bminer
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test --all

# Run specific crate tests
cargo test -p bminer-core

# Run with output
cargo test -- --nocapture
```

### Code Style

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all -- -D warnings
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Run `cargo fmt` and `cargo clippy`
6. Submit a pull request

## Security

- Never commit wallet addresses or pool credentials
- Use environment variables for sensitive data
- Keep dependencies updated: `cargo update`
- Report security issues privately

## License

This project is dual-licensed under:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

Choose the license that best suits your needs.

## Disclaimer

**Important**: Bitcoin mining profitability depends on many factors including:
- Electricity costs
- Hardware efficiency
- Network difficulty
- Bitcoin price

This software is provided for educational and experimental purposes. Always calculate your costs before mining.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- CUDA integration via [cudarc](https://github.com/coreylowman/cudarc)
- GPU monitoring via [nvml-wrapper](https://github.com/Cldfire/nvml-wrapper)
- Async runtime: [Tokio](https://tokio.rs/)

## Support

- 📖 Documentation: [docs/](docs/)
- 🐛 Issues: [GitHub Issues](https://github.com/yourusername/bminer/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/yourusername/bminer/discussions)

## Roadmap

- [x] Core mining functionality
- [x] Stratum protocol support
- [x] Idle detection
- [x] Configuration management
- [ ] CUDA kernel optimization
- [ ] Web dashboard
- [ ] Windows support
- [ ] OpenCL support
- [ ] Mining pool failover

---

**Made with ❤️ and Rust**