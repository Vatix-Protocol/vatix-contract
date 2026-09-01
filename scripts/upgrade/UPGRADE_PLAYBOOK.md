# Upgrade Playbook

Contract: `vatix-market-contract`  
Network: Stellar / Soroban  
Updated: 2026-09-01

---

## Overview

Soroban contracts are upgradeable via the `env.deployer().update_current_contract_wasm(new_wasm_hash)`
host function.  This playbook covers the pre-upgrade checks, execution steps,
post-upgrade validation, and rollback procedure for `vatix-market-contract`.

---

## Pre-upgrade checklist

- [ ] Run full test suite: `cd contracts/market && cargo test`
- [ ] Run clippy with `-D warnings`: `cargo clippy -- -D warnings`
- [ ] Audit storage layout changes (see [Storage Layout Compatibility](#storage-layout-compatibility))
- [ ] Confirm no `panic!` or `todo!()` on shipped code paths
- [ ] Confirm new entrypoints follow the timelock rule: any admin fee mutator must use `FEE_RATE_TIMELOCK_SECONDS = 172_800`
- [ ] Review `AUTH_TABLE.md` — any new entrypoints must be added
- [ ] Review `docs/events-reference.md` — any new events must be documented
- [ ] Review `docs/reentrancy-cei-audit.md` — any new mutating entrypoints must have a CEI note
- [ ] Review `docs/cross-contract-call-graph.md` — update if call graph changes
- [ ] Announce upgrade on governance channel with at least 48 h notice (mirrors timelock)
- [ ] Prepare rollback WASM hash (previous deployed version)

---

## Storage layout compatibility

The on-chain storage key schema is defined in `contracts/market/src/storage.rs`
via the `StorageKey` enum.  Because `StorageKey` is a `#[contracttype]`, its
XDR discriminants are **derived from the variant index by position**.

### Current key layout

| Variant | Discriminant | Type | Notes |
|---|---|---|---|
| `Market(u32)` | 0 | `Market` | Per-market state |
| `Position(u32, Address)` | 1 | `Position` | Per-(market, user) position |
| `Admin` | 2 | `Address` | Contract admin |
| `MarketCounter` | 3 | `u32` | Auto-increment counter |
| `FeeRateBps` | 4 | `u32` | Current fee rate |
| `PendingFeeRate` | 5 | `PendingFeeRate` | Queued fee-rate change |
| `FeeWaivers` | 6 | `Vec<Address>` | Bounded fee-waiver list |

### Rules

1. **Never reorder or delete variants** — this changes discriminants and
   corrupts existing storage reads.
2. **New variants must be appended** at the end of the enum.
3. **Never rename an existing variant** used as a key without a data migration.
4. **Struct fields may be added at the end** of `Market`, `Position`, or
   `PendingFeeRate` if the field has a sensible default and is read with
   `get()` + `unwrap_or` (not `expect`).

---

## Build steps

```bash
# 1. Build release WASM
cd contracts/market
cargo build --release --target wasm32-unknown-unknown

# 2. Locate output
ls -lh ../../target/wasm32-unknown-unknown/release/vatix_market_contract.wasm

# 3. Install WASM on network (produces new_wasm_hash)
soroban contract install \
  --wasm ../../target/wasm32-unknown-unknown/release/vatix_market_contract.wasm \
  --network $SOROBAN_NETWORK \
  --source $SOROBAN_ACCOUNT
```

---

## Upgrade execution

```bash
# Invoke update_current_contract_wasm via a dedicated admin entrypoint (post-MVP)
# Until an upgrade entrypoint is added, use soroban-cli directly:

soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  --source $ADMIN_ACCOUNT \
  -- \
  upgrade \
  --new_wasm_hash $NEW_WASM_HASH
```

> **Note:** An `upgrade` admin entrypoint is not yet implemented in the current
> ABI.  Until it is, use the Soroban CLI `contract update` command with the admin
> account directly.

---

## Post-upgrade validation

```bash
# 1. Verify new WASM hash is live
soroban contract info --id $CONTRACT_ADDRESS --network $SOROBAN_NETWORK

# 2. Read smoke-check values (should return unchanged state)
soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  -- get_fee_rate_bps

# 3. Read a known position to verify storage compatibility
soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  -- get_position \
  --market_id 1 \
  --user $KNOWN_USER_ADDRESS

# 4. Re-run integration tests against the live contract (if testnet)
cd contracts/market && cargo test
```

---

## Rollback procedure

If post-upgrade validation fails, redeploy the previous WASM hash:

```bash
soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  --source $ADMIN_ACCOUNT \
  -- \
  upgrade \
  --new_wasm_hash $PREVIOUS_WASM_HASH
```

Storage is not reverted by a WASM rollback — ensure the previous WASM is
forward-compatible with any storage writes the faulty new version may have
made.

---

## Fee rate change procedure (post-upgrade or routine)

The fee-rate change requires two admin transactions separated by 172 800 s (48 h):

```bash
# Step 1 — Queue change
soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  --source $ADMIN_ACCOUNT \
  -- queue_fee_rate_change \
  --caller $ADMIN_ADDRESS \
  --new_rate_bps 100

# Step 2 — Wait ≥ 48 h (172 800 ledger seconds)

# Step 3 — Apply change
soroban contract invoke \
  --id $CONTRACT_ADDRESS \
  --network $SOROBAN_NETWORK \
  --source $ADMIN_ACCOUNT \
  -- apply_pending_fee_rate \
  --caller $ADMIN_ADDRESS
```

The timelock is enforced on-chain via `env.ledger().timestamp()`.  Wall-clock
time is irrelevant; only the ledger sequence matters.

---

## Environment variables

| Variable | Description |
|---|---|
| `SOROBAN_NETWORK` | Network name (`testnet`, `mainnet`) |
| `SOROBAN_ACCOUNT` | Deployer account key name (from `soroban keys`) |
| `CONTRACT_ADDRESS` | Deployed contract address (`C...`) |
| `ADMIN_ACCOUNT` | Admin key name (must match stored `Admin` storage slot) |
| `ADMIN_ADDRESS` | Admin Stellar address (`G...`) |
| `NEW_WASM_HASH` | Hash from `soroban contract install` |
| `PREVIOUS_WASM_HASH` | Hash of the last known-good deployment |
