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

The script exits non-zero if any simulation step fails, making it suitable
as a pre-deploy CI gate.

---

## Pinned-hash verification (fail-closed)

[`scripts/upgrade/check-upgrade.sh`](../scripts/upgrade/check-upgrade.sh)
compares the freshly built WASM hash against the entries in
[`expected-hashes.json`](../expected-hashes.json). The check is
**fail-closed**:

- **Pinned entry** (a contract listed in `expected-hashes.json` with a
  non-empty `sha256`): the built hash **must** match exactly. Any mismatch —
  including a pinned entry whose expected hash is empty or missing — causes
  `check-upgrade.sh` to exit non-zero. There is no warning-only path and no
  silent Ed25519 fallback when adapters are enabled.
- **Unpinned entry** (contract absent from `expected-hashes.json`): the check
  reports the computed hash and exits zero. This is the default for local
development and for contracts that have not yet been promoted to a pinned
release.

Because the check is fail-closed, `scripts/upgrade-dry-run.sh` and the CI
`upgrade-dry-run` job propagate the non-zero exit and abort the pipeline.

### Intentionally pinning a contract

1. Build the release WASM and record its hash:

   ```bash
   bash scripts/verify-wasm-hash.sh contracts/market
   ```

2. Add or update the entry in `expected-hashes.json` with the full
   `sha256:<hex>` value. Do **not** leave the `sha256` field empty — an empty
   value on a pinned entry is treated as drift and fails the check.

3. Commit the updated `expected-hashes.json` alongside the WASM-affecting
   change so CI and reviewers see the pin in the same PR.

### Intentionally unpinning a contract

Remove the contract's entry from `expected-hashes.json` (or delete the file
entry entirely). The check will then report the computed hash without
failing. Unpinning is appropriate only for contracts that are not yet part of
a released, audited set; document the reason in the PR description.

---

## Checklist for contributors

Before opening a PR that touches storage:

- [ ] `STORAGE_VERSION` bumped (if storage layout changed)
- [ ] `MIGRATION.md` updated with what changed and why
- [ ] `StorageKey` enum and `lib.rs` doc table kept in sync
  (see [Reviewer Checklist](../contracts/market/STORAGE_MIGRATION_GUIDE.md#reviewer-checklist-storagekey-table-drift))
- [ ] `assert_version()` tests pass locally
- [ ] `expected-hashes.json` updated for any pinned contract whose WASM changed
- [ ] Dry-run simulation succeeds (`bash scripts/upgrade-dry-run.sh`)
- [ ] WASM hash documented in PR description

---

## Related documentation

- [Storage Migration Guide](../contracts/market/STORAGE_MIGRATION_GUIDE.md) —
  full procedures for testnet and mainnet, rollback plans, and common pitfalls.
- [Migration History](../contracts/market/MIGRATION.md) — per-version changelog.
- [Upgrade Playbook](../scripts/upgrade/UPGRADE_PLAYBOOK.md) — end-to-end
  upgrade runbook including the pinned-hash gate.
- [Build Verification](../README.md#build-verification) — how to confirm your
  WASM hash matches CI.
- [Testnet Smoke Test](../README.md#testnet-smoke-test) — lightweight
  read-only check after deployment.
