# BMiner Testing Guide - Braiins Pool Solo Mining

This guide will help you test BMiner with Braiins Pool's solo mining service.

## Prerequisites

1. **NVIDIA GPU** with CUDA support
2. **Bitcoin wallet address** (for receiving potential block rewards)
3. **Linux system** with CUDA drivers installed
4. **Rust toolchain** (1.75+)

## Quick Start

### 1. Build BMiner

```bash
cd /home/rmandrad/Development/bminer
cargo build --release
```

The binary will be at `target/release/bminer`

### 2. Configure for Braiins Pool

Edit the test configuration file:

```bash
nano config/braiins-pool-test.toml
```

**IMPORTANT:** Replace `YOUR_BITCOIN_ADDRESS_HERE` with your actual Bitcoin wallet address!

Example:
```toml
[pool]
url = "stratum+tcp://stratum.braiins.com:3333"
username = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"  # Your address
password = "x"
worker_name = "bminer-test"
```

### 3. Test GPU Detection

First, verify your GPU is detected:

```bash
./target/release/bminer --gpu-info
```

Expected output:
```
=== GPU Information ===

Device 0: NVIDIA GeForce RTX 3080
  Memory: 10 GB
  PCI Bus: 0000:01:00.0
  Temperature: 45°C
  Power: 50 W
  Utilization: 0%
```

### 4. Dry Run Test

Test configuration without actually mining:

```bash
./target/release/bminer --config config/braiins-pool-test.toml --dry-run
```

This will validate your configuration file.

### 5. Start Mining

```bash
./target/release/bminer --config config/braiins-pool-test.toml
```

Expected output:
```
INFO bminer: BMiner v0.1.0 starting...
INFO bminer: Configuration loaded successfully
INFO bminer::miner: Initialized hasher for GPU 0
INFO bminer: Starting BMiner...
INFO bminer::miner: Connecting to pool: stratum.braiins.com:3333
INFO bminer::miner: Connected to pool successfully
INFO bminer::miner: Subscribed successfully
INFO bminer::miner: Authorized successfully
INFO bminer::miner: Received new work: job_id=...
INFO bminer::miner: Mining: 0.50 MH/s, 0 shares (0.0% accepted)
```

### 6. Monitor Performance

The miner will log statistics every 30 seconds:

```
INFO bminer::miner: Mining: 105.23 MH/s, 42 shares (95.2% accepted)
```

### 7. Stop Mining

Press `Ctrl+C` to gracefully shutdown:

```
^C
INFO bminer: Received Ctrl+C, shutting down...
INFO bminer::miner: Stopping BMiner...
INFO bminer::miner: Disconnected from pool
```

## Understanding the Output

### Connection Messages
- `Connected to pool successfully` - TCP connection established
- `Subscribed successfully` - Stratum subscription complete
- `Authorized successfully` - Worker authenticated

### Mining Messages
- `Received new work` - New mining job from pool
- `Found valid share!` - Potential solution found
- `Share accepted!` - Pool accepted your share
- `Share rejected` - Pool rejected your share (check difficulty)

### Statistics
- **Hashrate**: Hashes per second (MH/s = million hashes/sec)
- **Shares submitted**: Total shares sent to pool
- **Acceptance rate**: Percentage of accepted shares

## Troubleshooting

### "No compatible NVIDIA GPUs found"

```bash
# Check NVIDIA drivers
nvidia-smi

# Verify CUDA installation
nvcc --version
```

### "Failed to connect"

```bash
# Test pool connectivity
telnet stratum.braiins.com 3333

# Check firewall
sudo ufw status
```

### "Authentication failed"

- Verify your Bitcoin address is valid
- Check the pool URL is correct
- Ensure you're using the right Stratum endpoint

### Low Hashrate

The current implementation uses CPU fallback for hashing. Expected performance:
- **CPU mining**: 0.1-1 MH/s (very slow)
- **GPU mining** (when CUDA kernels are optimized): 100+ MH/s

To improve performance, CUDA kernel optimization is needed (future enhancement).

### High Rejection Rate

- Check system time: `timedatectl status`
- Reduce mining intensity in config
- Check network latency to pool

## Monitoring in Braiins Pool Dashboard

1. Go to https://pool.braiins.com/
2. Enter your Bitcoin address
3. View your worker statistics
4. Check submitted shares and hashrate

## Important Notes

### Solo Mining Reality

⚠️ **Solo mining Bitcoin is extremely difficult!**

- Current network hashrate: ~400 EH/s (exahashes/second)
- Your GPU: ~0.0001 EH/s
- Probability of finding a block: ~1 in 4,000,000,000

Solo mining is primarily for:
- Testing and learning
- Supporting the network
- Lottery-style participation

### Electricity Costs

Calculate your costs before extended mining:
```
Daily cost = (GPU watts / 1000) × 24 hours × electricity rate
Example: (320W / 1000) × 24 × $0.12 = $0.92/day
```

### Pool Fees

Braiins Pool charges:
- **Solo mining**: 0% fee (you keep 100% of block reward if found)
- **Block reward**: 6.25 BTC + transaction fees (as of 2024)

## Advanced Testing

### Debug Mode

For detailed logging:

```bash
./target/release/bminer --config config/braiins-pool-test.toml --log-level debug
```

### Benchmark Mode

Test hashing performance without pool connection:

```bash
./target/release/bminer --benchmark
```

### Multiple GPUs

To use specific GPUs, edit config:

```toml
[gpu]
devices = [0, 1]  # Use GPU 0 and 1
```

## Next Steps

After successful testing:

1. **Optimize CUDA kernels** for better performance
2. **Join a mining pool** for regular payouts
3. **Monitor temperature** and adjust intensity
4. **Calculate profitability** vs electricity costs

## Support

- Check logs in console output
- Review configuration in `config/braiins-pool-test.toml`
- Verify GPU status with `--gpu-info`
- Test connectivity with `--dry-run`

## Disclaimer

This is experimental software. Solo mining Bitcoin is not profitable for most users. Use for educational purposes and testing only.

---

**Happy Mining! 🚀**