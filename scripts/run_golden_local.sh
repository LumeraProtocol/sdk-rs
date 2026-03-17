#!/usr/bin/env bash
set -euo pipefail

# Local golden test:
# register ticket -> upload -> download -> verify hash via examples/golden_devnet.rs

required_env=(LUMERA_MNEMONIC LUMERA_CREATOR)
for k in "${required_env[@]}"; do
  if [[ -z "${!k:-}" ]]; then
    echo "ERROR: $k is required"
    exit 1
  fi
done

export LUMERA_REST="${LUMERA_REST:-http://127.0.0.1:1317}"
export LUMERA_RPC="${LUMERA_RPC:-http://127.0.0.1:26657}"
export LUMERA_GRPC="${LUMERA_GRPC:-http://127.0.0.1:9090}"
export SNAPI_BASE="${SNAPI_BASE:-http://127.0.0.1:8080}"
export LUMERA_CHAIN_ID="${LUMERA_CHAIN_ID:-lumera-devnet}"
export GOLDEN_INPUT="${GOLDEN_INPUT:-/tmp/lumera-rs-golden-input.bin}"

cargo run --example golden_devnet
