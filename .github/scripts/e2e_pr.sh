#!/usr/bin/env bash
set -euo pipefail

# Production-grade PR E2E gate for sdk-rs:
# - spin fresh devnet
# - fix supernode endpoint mapping on-chain
# - start sn-api-server binary
# - run Rust SDK golden flow: register -> upload -> download -> hash match
# - emit explicit SUCCESS/FAIL and machine-readable report

DEVNET_TOOL_DIR="${DEVNET_TOOL_DIR:-/mnt/HC_Volume_104906218/agents/clients/lumera/ops/tools/lumera-devnet}"
DEVNET_RUN="${DEVNET_RUN:-$DEVNET_TOOL_DIR/devnet/run}"
SDK_RS_DIR="${SDK_RS_DIR:-$PWD}"
REPORT_DIR="${REPORT_DIR:-$SDK_RS_DIR/artifacts/e2e}"
SNAPI_PORT="${SNAPI_PORT:-8080}"
FORCE_FRESH="${FORCE_FRESH:-1}"
GOLDEN_INPUT="${GOLDEN_INPUT:-/tmp/cascade-e2e/golden-input-65536.bin}"
FILE_SIZE="${FILE_SIZE:-65536}"

mkdir -p "$REPORT_DIR"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_JSON="$REPORT_DIR/system-e2e-${TS}.json"
RUN_LOG="$REPORT_DIR/system-e2e-${TS}.log"

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN_LOG"; }
have_cmd() { command -v "$1" >/dev/null 2>&1; }

require_cmds() {
  local missing=0
  for c in jq curl lumerad sn-api-server; do
    if ! have_cmd "$c"; then
      echo "ERROR: required command missing: $c" | tee -a "$RUN_LOG"
      missing=1
    fi
  done

  if ! have_cmd cargo && [[ ! -x /root/.cargo/bin/cargo ]]; then
    echo "ERROR: required command missing: cargo" | tee -a "$RUN_LOG"
    missing=1
  fi

  if [[ ! -d "$DEVNET_TOOL_DIR" ]]; then
    echo "ERROR: DEVNET_TOOL_DIR not found: $DEVNET_TOOL_DIR" | tee -a "$RUN_LOG"
    missing=1
  fi

  if (( missing != 0 )); then
    return 1
  fi
}

cargo_bin() {
  if have_cmd cargo; then
    echo cargo
  else
    echo /root/.cargo/bin/cargo
  fi
}

chain_advancing() {
  local h1 h2
  h1="$(curl -sf http://127.0.0.1:26657/status | jq -r '.result.sync_info.latest_block_height' 2>/dev/null || echo 0)"
  sleep 3
  h2="$(curl -sf http://127.0.0.1:26657/status | jq -r '.result.sync_info.latest_block_height' 2>/dev/null || echo 0)"
  [[ "$h1" != "$h2" ]]
}

fix_supernode_ips() {
  log "Applying on-chain supernode endpoint fixes (IP:4444)"
  for i in $(seq 0 9); do
    local ip node_home val_addr sn_acct
    ip="127.0.0.$((i+2))"
    node_home="$DEVNET_RUN/chain/node${i}/lumerad"
    val_addr="$(lumerad keys show "node${i}" --bech val --keyring-backend test --home "$node_home" --output json | jq -r '.address')"
    sn_acct="$(jq -r ".supernodes[$i].identity" "$DEVNET_RUN/manifest.json")"

    lumerad tx supernode update-supernode "$val_addr" "${ip}:4444" "1.0.0" "$sn_acct" \
      --from "node${i}" --home "$node_home" --keyring-backend test \
      --chain-id lumera-devnet --node tcp://127.0.0.1:26657 \
      --fees 500ulume --gas auto --gas-adjustment 1.5 --yes >/dev/null 2>&1 || true
    sleep 1
  done
}

restart_snapi() {
  log "Restarting sn-api-server binary"
  for p in $(pgrep -f '^sn-api-server serve$' || true); do
    kill "$p" || true
  done
  sleep 1

  mkdir -p "$DEVNET_RUN/logs"
  GRPC_ADDR=127.0.0.1:9090 \
  CHAIN_ID=lumera-devnet \
  KEYRING_BACKEND=test \
  KEY_NAME=sn0 \
  BASE_DIR="$DEVNET_RUN/supernodes/sn0" \
  HTTP_PORT="$SNAPI_PORT" \
  nohup sn-api-server serve > "$DEVNET_RUN/logs/sn-api-server.log" 2>&1 &

  sleep 3
  curl -sf "http://127.0.0.1:${SNAPI_PORT}/api/v1/actions/cascade/tasks" >/dev/null
}

start_stack() {
  if [[ "$FORCE_FRESH" == "1" ]]; then
    log "Fresh devnet restart"
    make -C "$DEVNET_TOOL_DIR" clean-restart-core >/dev/null
  else
    if ! chain_advancing; then
      log "Chain not advancing; restarting core stack"
      make -C "$DEVNET_TOOL_DIR" clean-restart-core >/dev/null
    fi
  fi

  fix_supernode_ips
  restart_snapi
}

build_example() {
  local c
  c="$(cargo_bin)"
  log "Building golden_devnet example"
  (cd "$SDK_RS_DIR" && "$c" build --example golden_devnet)
}

run_golden() {
  local mnemonic creator out c
  c="$(cargo_bin)"
  mnemonic="$(jq -r '.secret' "$DEVNET_RUN/chain/node0/lumerad/key_seed.json")"
  creator="$(lumerad keys show node0 --keyring-backend test --home "$DEVNET_RUN/chain/node0/lumerad" --output json | jq -r '.address')"

  log "Running register->upload->download->hash-check"
  set +e
  out=$(LUMERA_REST=http://127.0.0.1:1317 \
    LUMERA_RPC=http://127.0.0.1:26657 \
    LUMERA_GRPC=http://127.0.0.1:9090 \
    SNAPI_BASE="http://127.0.0.1:${SNAPI_PORT}" \
    LUMERA_CHAIN_ID=lumera-devnet \
    LUMERA_CREATOR="$creator" \
    LUMERA_MNEMONIC="$mnemonic" \
    GOLDEN_INPUT="$GOLDEN_INPUT" \
    "$c" run --example golden_devnet 2>&1)
  code=$?
  set -e

  echo "$out" | tee -a "$RUN_LOG"

  action_id="$(printf "%s" "$out" | sed -n 's/^registered action_id=//p' | tail -n1)"
  upload_task="$(printf "%s" "$out" | sed -n 's/^upload task_id=//p' | tail -n1)"
  download_task="$(printf "%s" "$out" | sed -n 's/^download task_id=//p' | tail -n1)"
  hash="$(printf "%s" "$out" | sed -n 's/^GOLDEN_OK .* hash=//p' | tail -n1)"

  status="fail"
  if [[ $code -eq 0 && -n "$action_id" && -n "$upload_task" && -n "$download_task" && -n "$hash" ]]; then
    status="success"
  fi

  jq -n \
    --arg status "$status" \
    --arg action_id "${action_id:-}" \
    --arg upload_task "${upload_task:-}" \
    --arg download_task "${download_task:-}" \
    --arg hash "${hash:-}" \
    --arg log "$RUN_LOG" \
    --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '{status:$status, action_id:$action_id, upload_task:$upload_task, download_task:$download_task, hash:$hash, log:$log, timestamp:$at}' > "$REPORT_JSON"

  if [[ "$status" == "success" ]]; then
    log "SUCCESS action_id=$action_id upload_task=$upload_task download_task=$download_task hash=$hash"
    return 0
  fi

  log "FAIL register/upload/download/hash flow"
  return 1
}

main() {
  : > "$RUN_LOG"
  require_cmds

  mkdir -p "$(dirname "$GOLDEN_INPUT")"
  if [[ ! -f "$GOLDEN_INPUT" ]]; then
    head -c "$FILE_SIZE" /dev/urandom > "$GOLDEN_INPUT"
  fi

  start_stack
  build_example
  run_golden
}

main "$@"
