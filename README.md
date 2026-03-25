# Lumera Rust SDK (`lumera-sdk-rs`)

Official Rust SDK for integrating with Lumera chain actions and Cascade flows via `sn-api-server`.

This SDK covers the full action lifecycle used by apps and services:
- register ticket on-chain
- upload file via `sn-api-server`
- request and download file
- verify integrity by hash

---

## Table of Contents
- [What this SDK provides](#what-this-sdk-provides)
- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Quick start](#quick-start)
- [Golden test (what it means)](#golden-test-what-it-means)
- [Local developer commands (Makefile)](#local-developer-commands-makefile)
- [CI gates](#ci-gates)
- [Contributing / PR checklist](#contributing--pr-checklist)

---

## What this SDK provides

`lumera-sdk-rs` exposes 3 layers:

1. **Chain client (`chain`)**
   - action params + fee lookup
   - account info lookup
   - action registration transaction flow
   - generic tx build/sign/broadcast utilities
   - tx gas simulation helper + adjusted gas signing flow
   - tx confirmation lookup/wait helpers

2. **sn-api client (`snapi`)**
   - start upload
   - poll upload/download status
   - request download + fetch output file

3. **High-level SDK (`cascade`)**
   - deterministic payload/ID generation
   - register ticket on-chain
   - upload/download orchestration through `sn-api-server`

Also included:
- config loading from env, `.env`, `.toml`, `.json` via `config::SdkSettings`
- runnable end-to-end example: `examples/golden_devnet.rs`

---

## Requirements

- Rust stable (1.75+ recommended)
- Reachable Lumera endpoints (REST/RPC/gRPC)
- Reachable `sn-api-server`
- Signing keys for:
  - chain tx signing (`cosmrs::crypto::secp256k1::SigningKey`)
  - arbitrary payload signing (`k256::ecdsa::SigningKey`)

---

## Installation

```toml
[dependencies]
lumera-sdk-rs = "0.1"
```

From source:

```toml
[dependencies]
lumera-sdk-rs = { path = "../sdk-rs" }
```

---

## Configuration

Supported configuration styles:

- explicit in code (`CascadeConfig`, `ChainConfig`)
- environment variables (`SdkSettings::from_env()`)
- config file (`SdkSettings::from_file("*.toml|*.json")`)
- `.env` file (`SdkSettings::from_env_file(".env")`)

Common env vars:
- `LUMERA_CHAIN_ID`
- `LUMERA_GRPC`
- `LUMERA_RPC`
- `LUMERA_REST`
- `LUMERA_GAS_PRICE`
- `SNAPI_BASE`

---

## Quick start

See:
- `examples/golden_devnet.rs` (full E2E register/upload/download/hash)
- `examples/from_env_settings.rs` (load config from env + construct SDK)
- `examples/custom_config.rs` (explicit in-code endpoint config)
- `examples/README.md` (examples index)

`golden_devnet.rs` executes full flow:
1) register ticket
2) upload
3) request download
4) download file
5) compare hash of original vs downloaded bytes

---

## Simple Browser UI (sdk-rs API)

This repo now includes a minimal browser UI similar to `sdk-js/examples/browser`, but backed by `sdk-rs` API handlers:

- UI: `examples/ui/index.html`
- API server: `examples/ui_server.rs`

Run it:

```bash
cd sdk-rs
export LUMERA_MNEMONIC="..."
export LUMERA_CREATOR="lumera1..."
SNAPI_BASE=http://127.0.0.1:8089 cargo run --example ui_server
```

Then open:

```text
http://127.0.0.1:3002
```

Optional env:
- `SDK_RS_UI_PORT` (default `3002`)
- `SNAPI_BASE` (default `http://127.0.0.1:8089`)
- `LUMERA_MNEMONIC` (required for automatic register/sign flow)
- `LUMERA_CREATOR` (sender address; defaults to local dev value if omitted)
- `LUMERA_REST`, `LUMERA_RPC`, `LUMERA_GRPC`, `LUMERA_CHAIN_ID`

The UI calls Rust endpoints under `/api/*`, with no manual action/signature input:
- `POST /api/workflow/upload` (register ticket + sign + start upload)
- `GET /api/upload/{task_id}/summary`
- `POST /api/workflow/download` (uses last uploaded action by default)
- `GET /api/download/{task_id}/summary`
- `GET /api/download/{task_id}/file`

---

## Golden test (what it means)

In this repo, **golden test** means a full-system integration run against a real Lumera + `sn-api-server` stack, with strict pass criteria:

- action is registered on-chain successfully
- upload task succeeds
- download task succeeds
- downloaded file hash exactly matches original file hash

This is not a unit test and not a mock test. It is an end-to-end correctness gate for protocol compatibility.

Run locally:

```bash
make golden
```

`make golden` calls `scripts/run_golden_local.sh` and runs `cargo run --example golden_devnet`.

Required env for local golden run:
- `LUMERA_MNEMONIC`
- `LUMERA_CREATOR`

Optional (defaults provided):
- `LUMERA_REST` (default `http://127.0.0.1:1317`)
- `LUMERA_RPC` (default `http://127.0.0.1:26657`)
- `LUMERA_GRPC` (default `http://127.0.0.1:9090`)
- `SNAPI_BASE` (default `http://127.0.0.1:8080`)
- `LUMERA_CHAIN_ID` (default `lumera-devnet`)
- `GOLDEN_INPUT` (default `/tmp/lumera-rs-golden-input.bin`)

---

## Local developer commands (Makefile)

Use these before opening/updating a PR:

```bash
make fmt-check   # formatting gate
make lint        # clippy -D warnings
make test        # all tests
make doc         # docs with warnings denied
make check       # full local PR gate (fmt/lint/test/doc)
```

High-priority commands:

```bash
make build       # build all workspace targets
make check       # full quality gate
make golden      # full register/upload/download/hash test
make clean       # clean artifacts
```

For CI-parity PR E2E script against public testnet/public sn-api:

```bash
make e2e-pr
```

- Always runs public endpoint smoke checks.
- Runs full register/upload/download/hash flow when `LUMERA_MNEMONIC` and `LUMERA_CREATOR` are provided.

---

## CI gates

Current workflows enforce:

- `Lint + Format`
- `Test`
- `Docs`
- `Security (cargo audit)`
- `System E2E (PR, public sn-api)`:
  - runs on PR opened/reopened/synchronize
  - targets public endpoints (`snapi.testnet.lumera.io`, Lumera testnet RPC/REST/gRPC)
  - executes endpoint smoke checks for all PRs
  - executes full register → upload → download → hash match when testnet creds are configured as secrets
  - publishes run artifacts/logs

---

## Contributing / PR checklist

Before PR creation and before each push to an open PR:

```bash
make check
make golden
```

If your change affects chain/sn-api/cascade behavior, run:

```bash
make e2e-pr
```

PRs should include:
- clear scope and compatibility notes
- test evidence (at minimum `make check`; include golden/e2e evidence where relevant)
- no unrelated refactors

---

## Security notes

- never commit mnemonics/private keys
- inject secrets at runtime (env/secret manager)
- use dedicated keys per environment
