SHELL := /usr/bin/env bash

.PHONY: help build test test-unit fmt fmt-check lint doc check golden e2e-pr clean

help:
	@echo "Targets:"
	@echo "  make build       - Build all targets"
	@echo "  make test        - Run all tests"
	@echo "  make fmt         - Format code"
	@echo "  make fmt-check   - Check formatting"
	@echo "  make lint        - Run clippy with warnings denied"
	@echo "  make doc         - Build docs with warnings denied"
	@echo "  make check       - Run full local PR checks (fmt/lint/test/doc)"
	@echo "  make golden      - Run local golden example (register/upload/download/hash)"
	@echo "  make e2e-pr      - Run public sn-api PR E2E gate (testnet endpoints)"
	@echo "  make clean       - Clean cargo artifacts"

build:
	cargo build --workspace --all-features --all-targets

test: test-unit

test-unit:
	cargo test --workspace --all-features --all-targets

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

check: fmt-check lint test-unit doc

# Requires local lumera devnet + sn-api-server endpoints configured via env vars used by examples/golden_devnet.rs
golden:
	./scripts/run_golden_local.sh

# Public sn-api gate. Full flow requires LUMERA_MNEMONIC + LUMERA_CREATOR; otherwise runs smoke checks.
e2e-pr:
	./.github/scripts/e2e_public_pr.sh

clean:
	cargo clean
