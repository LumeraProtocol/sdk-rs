#!/usr/bin/env bash
set -euo pipefail

REPORT_DIR="${REPORT_DIR:-$PWD/artifacts/e2e}"
mkdir -p "$REPORT_DIR"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_LOG="$REPORT_DIR/public-e2e-${TS}.log"
REPORT_JSON="$REPORT_DIR/public-e2e-${TS}.json"

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN_LOG"; }

LUMERA_CHAIN_ID="${LUMERA_CHAIN_ID:-lumera-testnet-2}"
LUMERA_REST="${LUMERA_REST:-https://lcd.testnet.lumera.io}"
LUMERA_RPC="${LUMERA_RPC:-https://rpc.testnet.lumera.io}"
LUMERA_GRPC="${LUMERA_GRPC:-https://grpc.testnet.lumera.io}"
SNAPI_BASE="${SNAPI_BASE:-https://snapi.testnet.lumera.io}"
FILE_SIZE="${FILE_SIZE:-65536}"
GOLDEN_INPUT="${GOLDEN_INPUT:-/tmp/cascade-e2e/golden-input-65536.bin}"

: > "$RUN_LOG"

log "Public endpoint sanity checks"
curl -fsSL --retry 3 --retry-delay 2 "$SNAPI_BASE/api/v1/swagger/" >/dev/null
curl -fsSL --retry 3 --retry-delay 2 "$SNAPI_BASE/api/v1/actions/cascade/tasks" >/dev/null
curl -fsSL --retry 3 --retry-delay 2 "$LUMERA_RPC/status" >/dev/null
curl -fsSL --retry 3 --retry-delay 2 "$LUMERA_REST/cosmos/base/tendermint/v1beta1/node_info" >/dev/null

if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN=cargo
else
  CARGO_BIN=/root/.cargo/bin/cargo
fi

log "Build golden example"
"$CARGO_BIN" build --example golden_devnet | tee -a "$RUN_LOG"

mkdir -p "$(dirname "$GOLDEN_INPUT")"
if [[ ! -f "$GOLDEN_INPUT" ]]; then
  head -c "$FILE_SIZE" /dev/urandom > "$GOLDEN_INPUT"
fi

status="smoke_success"
mode="smoke"
action_id=""
upload_task=""
download_task=""
hash=""

if [[ -n "${LUMERA_MNEMONIC:-}" && -n "${LUMERA_CREATOR:-}" ]]; then
  log "Running full golden flow against public sn-api + testnet"
  mode="full"
  set +e
  out=$(LUMERA_REST="$LUMERA_REST" \
    LUMERA_RPC="$LUMERA_RPC" \
    LUMERA_GRPC="$LUMERA_GRPC" \
    SNAPI_BASE="$SNAPI_BASE" \
    LUMERA_CHAIN_ID="$LUMERA_CHAIN_ID" \
    LUMERA_CREATOR="$LUMERA_CREATOR" \
    LUMERA_MNEMONIC="$LUMERA_MNEMONIC" \
    GOLDEN_INPUT="$GOLDEN_INPUT" \
    "$CARGO_BIN" run --example golden_devnet 2>&1)
  code=$?
  set -e

  echo "$out" | tee -a "$RUN_LOG"

  action_id="$(printf "%s" "$out" | sed -n 's/^registered action_id=//p' | tail -n1)"
  upload_task="$(printf "%s" "$out" | sed -n 's/^upload task_id=//p' | tail -n1)"
  download_task="$(printf "%s" "$out" | sed -n 's/^download task_id=//p' | tail -n1)"
  hash="$(printf "%s" "$out" | sed -n 's/^GOLDEN_OK .* hash=//p' | tail -n1)"

  if [[ $code -eq 0 && -n "$action_id" && -n "$upload_task" && -n "$download_task" && -n "$hash" ]]; then
    status="success"
    log "SUCCESS action_id=$action_id upload_task=$upload_task download_task=$download_task hash=$hash"
  else
    status="fail"
    log "FAIL full e2e flow"
  fi
else
  log "LUMERA_MNEMONIC/LUMERA_CREATOR not provided; smoke checks passed"
fi

jq -n \
  --arg status "$status" \
  --arg mode "$mode" \
  --arg chain_id "$LUMERA_CHAIN_ID" \
  --arg rpc "$LUMERA_RPC" \
  --arg rest "$LUMERA_REST" \
  --arg grpc "$LUMERA_GRPC" \
  --arg snapi "$SNAPI_BASE" \
  --arg action_id "$action_id" \
  --arg upload_task "$upload_task" \
  --arg download_task "$download_task" \
  --arg hash "$hash" \
  --arg log "$RUN_LOG" \
  --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{status:$status, mode:$mode, chain_id:$chain_id, rpc:$rpc, rest:$rest, grpc:$grpc, snapi:$snapi, action_id:$action_id, upload_task:$upload_task, download_task:$download_task, hash:$hash, log:$log, timestamp:$at}' > "$REPORT_JSON"

[[ "$status" != "fail" ]]
