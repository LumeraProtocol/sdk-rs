# sdk-rs chain expansion feasibility (vs sdk-go)

## Scope requested
1) Safe/unified key handling for tx signing + message signing.
2) Expand beyond Cascade into stronger chain interactions (account info, fees, tx sending, tx confirmation).

## Feasibility verdict
**Feasible now with current Rust stack** (`cosmrs`, `tendermint-rpc`, `reqwest`).
No blocker found for parity on the requested baseline.

## What exists in sdk-rs today
- Action params and action-fee query (REST).
- Register-action tx flow with signing and sequence retry.
- sn-api upload/download orchestration.

## Gaps relative to sdk-go baseline
- No dedicated identity/key module (key derivation duplicated in examples).
- No explicit signer/address mismatch guard in tx path.
- Fee handling in tx path was static/fixed (not parsed from configured gas price).
- No generic tx lookup + wait-for-confirmation utility.
- No explicit account-info public API.

## What was implemented in this step
- Added `src/keys.rs`:
  - `SigningIdentity::from_mnemonic(...)` (single source for tx + arbitrary signing keys).
  - `validate_address(...)` and `validate_chain_prefix(...)`.
  - `derive_signing_keys_from_mnemonic(...)` helper for existing flows.
- Updated `src/chain.rs`:
  - Added signer-vs-creator precheck (`validate_signer_matches_creator`).
  - Added `get_account_info(address)`.
  - Added `calculate_fee_amount(gas_limit)` from configured gas price.
  - Replaced fixed tx fee with gas-price-derived fee.
  - Added `get_tx(tx_hash)` and `wait_for_tx_confirmation(tx_hash, timeout_secs)`.
  - Added generic tx path: `build_signed_tx(...)`, `broadcast_signed_tx(...)`, `send_any_msgs(...)`.
  - Added gas simulation + adjusted signing flow: `simulate_gas_for_tx(...)`, `build_signed_tx_with_simulation(...)`.
  - Added common Lumera wrapper: `request_action_tx(...)`.
  - Added broadcast mode support: `Async`, `Sync`, `Commit`.
- Refactored examples (`golden_devnet`, `ui_server`) to use centralized key derivation helper.

## Next parity steps (recommended)
1. Add richer tx result/event extraction helpers for action/general events.
2. Add integration tests for signer/address mismatch and tx wait timeout behavior.
3. Add convenience wrappers for more common Lumera msgs on top of generic tx path.

## Validation done
- `cargo check`: pass
- `cargo test --lib`: pass (13 tests)
