# Upgrade Dry-Run Guide

> **Purpose:** Validate a contract upgrade and its storage migrations in a safe,
> non-destructive way before applying changes to testnet or mainnet.

---

## Overview

A "dry-run" upgrade simulates the `stellar contract deploy` + `initialize` flow
against a locally-forked or simulation-mode ledger so you can confirm:

- The new WASM builds and passes the host's size/fee checks.
- The on-chain storage version is incremented correctly.
- `assert_version()` rejects the old layout and accepts the new one.
- No existing storage keys silently disappear or change meaning.

This guide covers the full dry-run workflow using the Stellar CLI's
`--send=no` (simulate-only) flag, a local testnet fork, and the provided
script stub.

---

## Prerequisites

| Tool | Minimum version | Install |
|------|----------------|---------|
| `stellar` CLI | v21.4.0+ | [developers.stellar.org/docs/tools/cli](https://developers.stellar.org/docs/tools/cli) |
| Rust toolchain | stable | `rustup update stable` |
| `wasm32v1-none` target | — | `rustup target add wasm32v1-none` |
| Funded testnet account | — | [friendbot.stellar.org](https://friendbot.stellar.org) |

---

## Step-by-step dry-run procedure

### 1. Bump the storage version (if required)

Before building, verify whether the storage layout changed.  
Follow the **[Reviewer Checklist: StorageKey Table Drift](../contracts/market/STORAGE_MIGRATION_GUIDE.md#reviewer-checklist-storagekey-table-drift)** and, if needed, increment `STORAGE_VERSION` in `contracts/market/src/storage.rs`.

```bash
# Quick check: list StorageKey variants
grep -oE '^\s{4}[A-Z][A-Za-z]+' contracts/market/src/storage.rs | sort -u

# Current version
grep 'STORAGE_VERSION' contracts/market/src/storage.rs
```

Document the change in [`contracts/market/MIGRATION.md`](../contracts/market/MIGRATION.md).

### 2. Build the upgraded WASM

```bash
cd contracts/market
stellar contract build
# Artifact: ../../target/wasm32v1-none/release/vatix_market_contract.wasm
```

Verify the hash matches what CI produces:

```bash
bash scripts/verify-wasm-hash.sh contracts/market
```

### 3. Simulate the upload (no broadcast)

```bash
stellar contract upload \
  --wasm target/wasm32v1-none/release/vatix_market_contract.wasm \
  --source $TESTNET_SECRET_KEY \
  --network testnet \
  --send=no
```

`--send=no` runs full simulation (fee estimation, preflight, host-level
validation) without submitting the transaction. A clean exit here confirms
the WASM is within byte-size and cost limits.

### 4. Simulate the upgrade call

```bash
stellar contract invoke \
  --id $OLD_CONTRACT_ID \
  --source $TESTNET_SECRET_KEY \
  --network testnet \
  --send=no \
  -- upgrade \
  --new_wasm_hash <HASH_FROM_STEP_3>
```

Again, `--send=no` exercises the `upgrade` entry-point's preflight without
mutating ledger state.

### 5. Verify storage version acceptance

After a real upgrade (not dry-run), call a read function that routes through
`assert_version()` — `get_admin` is a simple, safe choice:

```bash
stellar contract invoke \
  --id $NEW_CONTRACT_ID \
  --network testnet \
  --send=no \
  -- get_admin
```

A successful return confirms the new version is written and accepted.  
An `UpgradeRequired` error (#70) means `initialize` or the migration step
was not run yet.

### 6. Confirm old deployment is locked (testnet only)

```bash
stellar contract invoke \
  --id $OLD_CONTRACT_ID \
  --network testnet \
  --send=no \
  -- get_admin
# Expected: Error(Contract, #70) — UpgradeRequired
```

This verifies that old clients cannot continue calling the stale contract.

---

## Automated dry-run script

[`scripts/upgrade-dry-run.sh`](../scripts/upgrade-dry-run.sh) wraps the
steps above in a single command. It is intentionally simulation-only
(`--send=no` throughout) and safe to run in CI or locally against a funded
testnet account.

```bash
# Required env vars
export TESTNET_SECRET_KEY="S..."
export OLD_CONTRACT_ID="C..."   # existing deployment to verify lockout
export CONTRACT_DIR="contracts/market"   # default

bash scripts/upgrade-dry-run.sh
```

**Fail-closed behavior (issue #762):** Both `TESTNET_SECRET_KEY` and
`OLD_CONTRACT_ID` are required.  The script exits non-zero immediately if
either is missing — it will **not** silently skip steps and return a
false-green exit code.  This ensures the upgrade and version-lockout
simulation steps are never bypassed by accident in a CI gate.

The script also exits non-zero if any simulation step fails, making it
suitable as a pre-deploy CI gate.

---

## Checklist for contributors

Before opening a PR that touches storage:

- [ ] `STORAGE_VERSION` bumped (if storage layout changed)
- [ ] `MIGRATION.md` updated with what changed and why
- [ ] `StorageKey` enum and `lib.rs` doc table kept in sync
  (see [Reviewer Checklist](../contracts/market/STORAGE_MIGRATION_GUIDE.md#reviewer-checklist-storagekey-table-drift))
- [ ] `assert_version()` tests pass locally
- [ ] Dry-run simulation succeeds (`bash scripts/upgrade-dry-run.sh`)
- [ ] WASM hash documented in PR description

---

## Related documentation

- [Storage Migration Guide](../contracts/market/STORAGE_MIGRATION_GUIDE.md) —
  full procedures for testnet and mainnet, rollback plans, and common pitfalls.
- [Migration History](../contracts/market/MIGRATION.md) — per-version changelog.
- [Build Verification](../README.md#build-verification) — how to confirm your
  WASM hash matches CI.
- [Testnet Smoke Test](../README.md#testnet-smoke-test) — lightweight
  read-only check after deployment.
