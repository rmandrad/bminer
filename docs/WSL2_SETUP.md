# BMiner WSL2 Setup Guide

This guide helps you set up BMiner on Windows Subsystem for Linux 2 (WSL2) with NVIDIA GPU support.

## Prerequisites

✅ **You already have:**
- Windows 11 with WSL2 installed
- NVIDIA GPU (RTX 2000 Ada Generation detected!)
- NVIDIA drivers installed (595.97)
- CUDA 13.2 support

## Quick Setup (One-Time)

### 1. Create NVML Symlink

The NVML library needs a symlink for BMiner to find it:

```bash
sudo ln -sf /usr/lib/wsl/lib/libnvidia-ml.so.1 /usr/lib/wsl/lib/libnvidia-ml.so
```

✅ **Done!** This only needs to be done once.

### 2. Verify GPU Detection

Test that BMiner can see your GPU:

```bash
cd /home/rmandrad/Development/bminer
./bminer.sh --gpu-info
```

You should see:
```
=== GPU Information ===

Device 0: NVIDIA RTX 2000 Ada Generation Laptop GPU
  Memory: 7 GB
  PCI Bus: 0000:01:00.0
  Temperature: 46°C
  Power: 5 W
  Utilization: 0%
```

## Running BMiner

### Easy Way (Recommended)

Use the provided launcher script that handles library paths automatically:

```bash
# Check GPU info
./bminer.sh --gpu-info

# Create a local test config from the example
cp config/braiins-pool-test.toml.example config/braiins-pool-test.toml

# Start mining
./bminer.sh --config config/braiins-pool-test.toml

# Show help
./bminer.sh --help
```

### Manual Way

If you prefer to run the binary directly:

```bash
LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH ./target/release/bminer --config config/braiins-pool-test.toml
```

## Configuration

### 1. Get a Bitcoin Address

Follow the guide in `docs/BITCOIN_WALLET_SETUP.md` to get a Bitcoin address.

**Quick option for testing:**
```bash
# Use the demo address already in the config
cp config/braiins-pool-test.toml.example config/braiins-pool-test.toml
./bminer.sh --config config/braiins-pool-test.toml
```

### 2. Edit Configuration

```bash
cp config/braiins-pool-test.toml.example config/braiins-pool-test.toml
nano config/braiins-pool-test.toml
```

Replace `YOUR_BITCOIN_ADDRESS_HERE` with your actual Bitcoin address (starts with `bc1...`).

### 3. Start Mining

```bash
./bminer.sh --config config/braiins-pool-test.toml
```

## Troubleshooting

### Error: "Unable to dynamically load the cuda shared library"

**Solution:** Use the `bminer.sh` launcher script instead of running the binary directly.

```bash
# ❌ Don't do this:
./target/release/bminer --config config.toml

# ✅ Do this instead:
./bminer.sh --config config.toml
```

### Error: "libnvidia-ml.so: cannot open shared object file"

**Solution:** Create the symlink (see step 1 above):

```bash
sudo ln -sf /usr/lib/wsl/lib/libnvidia-ml.so.1 /usr/lib/wsl/lib/libnvidia-ml.so
```

### GPU Not Detected

**Check NVIDIA drivers:**
```bash
nvidia-smi
```

You should see your GPU listed. If not, install NVIDIA drivers for WSL2:
- https://docs.nvidia.com/cuda/wsl-user-guide/index.html

### Permission Denied

**Make the launcher executable:**
```bash
chmod +x bminer.sh
```

## WSL2-Specific Notes

### Library Paths

WSL2 stores NVIDIA libraries in `/usr/lib/wsl/lib/`:
- `libcuda.so` - CUDA runtime
- `libnvidia-ml.so` - NVML (GPU monitoring)

The `bminer.sh` script automatically adds this path to `LD_LIBRARY_PATH`.

### Performance

WSL2 GPU performance is typically 90-95% of native Linux performance. This is excellent for Bitcoin mining!

### Power Management

Your GPU will automatically manage power and thermals. Monitor with:
```bash
watch -n 1 nvidia-smi
```

### Windows Integration

You can access BMiner from Windows:
```powershell
# From PowerShell/CMD
wsl ./bminer.sh --gpu-info
```

## Testing Checklist

- [ ] NVML symlink created
- [ ] GPU detected with `./bminer.sh --gpu-info`
- [ ] Bitcoin address obtained (or using demo address)
- [ ] Config file edited with your address
- [ ] BMiner starts without errors
- [ ] Connected to Braiins Pool
- [ ] Shares being submitted

## Next Steps

1. **Monitor Performance:**
   ```bash
   # In another terminal
   watch -n 1 nvidia-smi
   ```

2. **Check Pool Dashboard:**
   - Go to https://pool.braiins.com/
   - Search for your Bitcoin address
   - View your mining statistics

3. **Let it Run:**
   - BMiner will mine when your system is idle
   - Press Ctrl+C to stop gracefully

## Support

- **BMiner Issues:** Check `README.md` and `TESTING.md`
- **WSL2 GPU Issues:** https://docs.nvidia.com/cuda/wsl-user-guide/
- **Bitcoin Wallet:** See `docs/BITCOIN_WALLET_SETUP.md`

## Summary

✅ **Your System:**
- OS: WSL2 on Windows 11
- GPU: NVIDIA RTX 2000 Ada Generation (7 GB)
- Driver: 595.97
- CUDA: 13.2

✅ **Setup Complete:**
- NVML symlink created
- Launcher script ready (`bminer.sh`)
- GPU detection working
- Ready to mine!

**Start mining now:**
```bash
./bminer.sh --config config/braiins-pool-test.toml
```

Happy mining! 🚀
