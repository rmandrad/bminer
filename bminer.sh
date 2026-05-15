#!/bin/bash
# BMiner launcher script for WSL2/Linux
# This script sets up common CUDA/NVML library paths.

append_ld_library_path() {
    local candidate="$1"
    if [[ -d "$candidate" ]] && [[ ":${LD_LIBRARY_PATH:-}:" != *":$candidate:"* ]]; then
        if [[ -n "${LD_LIBRARY_PATH:-}" ]]; then
            export LD_LIBRARY_PATH="$candidate:$LD_LIBRARY_PATH"
        else
            export LD_LIBRARY_PATH="$candidate"
        fi
    fi
}

append_ld_library_path "/usr/lib/wsl/lib"
append_ld_library_path "/opt/cuda/lib64"
append_ld_library_path "/opt/cuda/targets/x86_64-linux/lib"
append_ld_library_path "/usr/local/cuda/lib64"
append_ld_library_path "/usr/local/cuda-12/lib64"
append_ld_library_path "/usr/local/cuda-12.5/lib64"

if [[ -n "${BMINER_CUDA_LIB_DIR:-}" ]]; then
    append_ld_library_path "$BMINER_CUDA_LIB_DIR"
fi

# Run bminer with all arguments passed to this script
exec "$(dirname "$0")/target/release/bminer" "$@"

# Made with Bob
