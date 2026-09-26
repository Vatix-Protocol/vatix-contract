# Vatix Protocol Upgrade Playbook

This document defines the strict order and preconditions for cross-contract upgrades in the Vatix protocol.

## Critical Safety Requirement: Upgrade Order

**ABSOLUTE RULE**: Upgrades to enable oracle adapters must follow this exact order. Deviation **will brick share minting**.

### Upgrade Sequence

1. **Outcome Token Contract** (initializes first)
   - Status: Ready for oracle adapter integration
   - Action: Set oracle adapter registry reference
   - Side effects: None on market contract

2. **Treasury Contract** (initializes second)
   - Status: Ready for oracle adapter integration
   - Action: Register fee routes to adapters
   - Side effects: None on market contract

3. **Resolution Contract** (initializes third)
   - Status: To be implemented
   - Action: Register supported oracle adapters (Reflector, Pyth, etc.)
   - Side effects: Signals to market contract that adapters are live

4. **Market Contract** (final)
   - Status: Ready to lock oracle mode
   - Action: Resolution contract calls `enable_oracle_adapters()` on market contract
   - Side effects: Ed25519 fallback permanently disabled for this contract instance

### What Each Step Does

#### Step 3 → Step 4 Handoff: Oracle Adapter Enablement

```rust
// Resolution contract triggers this after adapter registration
market_contract.enable_oracle_adapters()

// Inside market contract storage:
storage::enable_oracle_adapters(env)  // Sets persistent flag

// From this point forward:
if storage::has_oracle_adapters(env) {
    // Ed25519 verification ALWAYS fails
    return Err(ContractError::UnauthorizedOracle)
}
```

This creates a **fail-closed** security model:
- If oracle adapters are enabled, Ed25519 is **rejected immediately**
- Prevents silent fallback during incomplete deployments
- Makes incomplete upgrades detectable (resolution fails, not silently succeeds)

## Why This Order Matters

### Scenario: Wrong Order (Steps 2→4 only, skipping 3)

```
Step 1 ✅ Outcome Token: adapters registered
Step 2 ✅ Treasury: fee routes configured
Step 4 ❌ Market: enable_oracle_adapters() called TOO EARLY

Problem: No resolution contract has registered adapters yet!
Result: Markets cannot be resolved via adapters (fallback to Ed25519)
Impact: Trading continues with old oracle model, defeating upgrade
```

### Scenario: Correct Order

```
Step 1 ✅ Outcome Token: adapters registered
Step 2 ✅ Treasury: fee routes configured
Step 3 ✅ Resolution: adapters registered (Reflector, Pyth)
Step 4 ✅ Market: enable_oracle_adapters() called NOW SAFE

All systems aligned: New oracle model active, Ed25519 disabled
```

## Storage Versioning

The market contract's persistent storage uses versioned keys to support future upgrades:

```rust
#[contracttype]
pub enum StorageKey {
    Market(u32),              // v1: Market state
    Position(u32, Address),   // v1: User positions
    Admin,                    // v1: Admin address
    MarketCounter,            // v1: Market ID counter
    OracleAdapters,           // v2: Oracle adapter flag (NEW)
}
```

**Migration strategy for future versions**:
1. Add new `StorageKey` variant (e.g., `OracleAdaptersV2`)
2. Add migration function to copy data
3. Update storage getters to prefer new key, fall back to old

This ensures contracts can be upgraded without losing state.

## Deployment Checklist

Before enabling oracle adapters in production:

- [ ] All four contracts compiled and verified
- [ ] Resolution contract has ≥1 registered adapter (Reflector or Pyth)
- [ ] Market contract has `enable_oracle_adapters()` marked as callable only by authorized account
- [ ] Testnet deployment successful with adapter-to-market resolution flow
- [ ] Mainnet rehearsal: deploy in order, verify resolution works
- [ ] Audit sign-off on upgrade procedure and fail-closed guarantees

## Rollback Procedure

If oracle adapters fail to activate correctly:

1. **Do NOT upgrade market contract alone** — this will disable Ed25519
2. Resolution contract upgrade can be rolled back independently
3. Market contract can be redeployed with `OracleAdapters` key removed from storage
4. Outcome token and treasury are not affected

---

## References

- Market contract oracle module: [contracts/market/src/oracle.rs](../../contracts/market/src/oracle.rs)
- Storage versioning: [contracts/market/src/storage.rs](../../contracts/market/src/storage.rs)
- Fail-closed security model: [docs/SECURITY.md](../../docs/SECURITY.md)
- Issue tracking: [#139 - Decentralized Oracle Integration](https://github.com/Vatix-Protocol/vatix-contract/issues/139)
# Cross-Contract Upgrade Playbook

> **Executable upgrade safety net for the four interdependent Vatix contracts
> (Market, Treasury, Resolution, Outcome Token).** Complements — does not
> replace — the single-contract guides:
> [`contracts/market/STORAGE_MIGRATION_GUIDE.md`](../../contracts/market/STORAGE_MIGRATION_GUIDE.md).

## Why this exists

Upgrading Market, Treasury, Resolution, and Outcome Token requires an
ordered deploy, WASM hash verification, and storage-version coordination
across all four crates. A partial or out-of-order upgrade can:

- Brick the `finalize → resolve_market` callback (Resolution → Market) or
  fee collection (Market → Treasury) if one contract is upgraded without
  re-wiring its counterpart's address.
- Cause `ContractError::UpgradeRequired` / `TreasuryError::UpgradeRequired`
  reads on a contract whose on-chain storage version doesn't match the
  deployed code, while writers on a different deployment use the new layout.
- Turn a mainnet deploy that was never rehearsed on testnet into an
  existential risk — see "Rollback and Recovery" in the market storage guide.

This playbook ties together the four contracts' deploy order, version
compatibility, and rollback procedure into one scripted, testable dry-run.

## Table of contents

1. [Deploy order](#deploy-order)
2. [WASM hash pinning](#wasm-hash-pinning)
3. [Storage version compatibility matrix](#storage-version-compatibility-matrix)
4. [Dual-read migration for the next storage bump](#dual-read-migration-for-the-next-storage-bump)
5. [Running the dry-run](#running-the-dry-run)
6. [Staging dry-run checklist](#staging-dry-run-checklist)
7. [CI enforcement](#ci-enforcement)
8. [Rollback](#rollback)

---

## Deploy order

Derived from the registration prerequisites in
[`docs/cross-contract-call-graph.md`](../../docs/cross-contract-call-graph.md#registration-prerequisites).
All cross-contract wiring is opt-in and admin-controlled — nothing calls out
to another contract until its address is explicitly registered — but
`Resolution::initialize` takes the Market contract's address as a
constructor argument, so Market must exist first.

1. **Deploy Market.** No dependencies on the other three contracts at
   deploy time.
2. **Deploy Treasury.**
3. **Deploy Outcome Token.**
4. **Deploy Resolution**, passing the Market contract address to
   `initialize(admin, factory, market_contract)`.
5. **Wire the four together** via admin calls (safe to run in any order once
   all four are deployed):
   ```
   MarketContract::set_treasury(admin, treasury_address)
   MarketContract::set_fee_rate(admin, fee_rate_bps)
   MarketContract::set_outcome_token_contract(admin, outcome_token_address)
   MarketContract::set_resolution_contract(admin, resolution_address)
   OutcomeTokenContract::set_market_contract(admin, market_address)
   TreasuryContract::initialize(admin, market_address)   # or set_market_contract
   ```
6. **Record every contract ID** in `deployments/testnet.json` (see
   [`deployments/README.md`](../../deployments/README.md)) before enabling
   traffic — this is also what [`rollback.sh`](rollback.sh) reads to recover
   a previous deployment.

Until step 5 completes for a given pairing, calls that depend on that wiring
fail closed (e.g. `withdraw_unused_collateral` simply skips fee routing if no
treasury is registered) rather than silently succeeding against a stale
address.

## WASM hash pinning

Every upgrade must pin the exact WASM hash it deploys. `check-upgrade.sh`
verifies the on-chain WASM hash of each contract against the hash recorded in
`deployments/<network>.json` before any wiring call is allowed to proceed.

- A hash mismatch is a **hard failure** (exit non-zero) — never a warning.
- Missing hash entries are treated as drift and fail closed.
- The check is idempotent: re-running it against an already-verified
  deployment produces the same result and performs no writes.

## Storage version compatibility matrix

| Contract       | Storage version | Compatible code versions |
| -------------- | --------------- | ------------------------ |
| Market         | v2              | ≥ v2                     |
| Treasury       | v1              | ≥ v1                     |
| Resolution     | v1              | ≥ v1                     |
| Outcome Token  | v1              | ≥ v1                     |

A deployed contract whose on-chain storage version is **lower** than the
version required by the code being deployed must fail the upgrade check
(`UpgradeRequired`) rather than proceed.

## Dual-read migration for the next storage bump

When bumping a storage version, land a dual-read window first: readers prefer
the new key and fall back to the old key, writers write both. Only after the
dual-read window is verified on testnet may the old key be dropped.

## Running the dry-run

```
./scripts/upgrade/check-upgrade.sh --network testnet
```

The script is **fail-closed**: it exits non-zero on any of the following,
and never reports success on a partial or unverifiable state.

| Condition                                   | Exit code | Error code            |
| ------------------------------------------- | --------- | --------------------- |
| Missing/invalid `--network` or config       | 2         | `E_UPGRADE_INPUT`     |
| Missing deployment record / contract ID     | 3         | `E_UPGRADE_MISSING`   |
| WASM hash mismatch (drift)                  | 4         | `E_UPGRADE_DRIFT`     |
| Storage version too low (`UpgradeRequired`) | 5         | `E_UPGRADE_VERSION`   |
| RPC/dependency outage                       | 6         | `E_UPGRADE_DEPENDENCY`|
| Replayed / concurrent run detected          | 7         | `E_UPGRADE_REPLAY`    |

Each run emits a correlation id (from `UPGRADE_CORRELATION_ID` or a generated
UUID) so ops can trace a check across logs without leaking secrets. The check
performs **no writes** and is safe to re-run; a lock file keyed by network +
correlation id makes concurrent runs fail closed with `E_UPGRADE_REPLAY`
rather than racing.

## Staging dry-run checklist

- [ ] `check-upgrade.sh --network testnet` exits 0 against the current testnet deployment.
- [ ] Deliberately corrupt one WASM hash in `deployments/testnet.json`; confirm exit 4 (`E_UPGRADE_DRIFT`).
- [ ] Remove one contract ID; confirm exit 3 (`E_UPGRADE_MISSING`).
- [ ] Point at an unreachable RPC; confirm exit 6 (`E_UPGRADE_DEPENDENCY`).
- [ ] Re-run twice concurrently; confirm the second exits 7 (`E_UPGRADE_REPLAY`).
- [ ] Confirm no secrets appear in stdout/stderr or logs.

## CI enforcement

`check-upgrade.sh` runs as a **required** check in
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) on every PR that
touches `contracts/**`, `scripts/upgrade/**`, or `deployments/**`. Because the
check is fail-closed, a red check blocks merge — there is no path to merge an
upgrade that fails hash, version, or dependency verification.

## Rollback

See [`rollback.sh`](rollback.sh) and the "Rollback and Recovery" section of
[`contracts/market/STORAGE_MIGRATION_GUIDE.md`](../../contracts/market/STORAGE_MIGRATION_GUIDE.md).
Rollback reads the previous contract IDs from `deployments/<network>.json`;
never roll back Market alone while oracle adapters are enabled.
