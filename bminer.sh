#!/bin/bash
# BMiner launcher script for WSL2
# This script sets up the correct library paths for CUDA/NVML

export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH

# Run bminer with all arguments passed to this script
exec "$(dirname "$0")/target/release/bminer" "$@"

# Made with Bob
