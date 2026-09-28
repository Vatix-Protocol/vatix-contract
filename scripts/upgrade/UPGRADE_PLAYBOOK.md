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

## Storage Compatibility Matrix

The matrix below is the single source of truth for which storage keys are
upgrade-safe, which require a migration, and how each contract fails closed
when the on-chain storage version does not match the deployed code. It is
kept consistent with the per-contract guides:
[`contracts/market/STORAGE_MIGRATION_GUIDE.md`](../../contracts/market/STORAGE_MIGRATION_GUIDE.md),
[`contracts/market/STORAGE_VERSION_CI_CHECK.md`](../../contracts/market/STORAGE_VERSION_CI_CHECK.md),
and [`MIGRATION.md`](../../MIGRATION.md). If this matrix and those documents
ever disagree, treat the per-contract guide as authoritative and open a PR to
re-sync this table.

### Version scheme

- Each contract stores a `StorageVersion` value under a dedicated key
  (`DataKey::StorageVersion` / `StorageKey::Version`).
- The version is a monotonically increasing `u32`. It is written **once** on
  `initialize` and only advanced by an explicit migration entrypoint.
- Readers compare the stored version against the code's expected version:
  - `stored == expected` → normal read path.
  - `stored < expected` → the contract returns
    `ContractError::UpgradeRequired` (fail-closed) until the migration runs.
  - `stored > expected` → the contract returns
    `ContractError::IncompatibleStorageVersion` (fail-closed); this means the
    code is older than the data and must not write.
- Writes are **always** gated on `stored == expected`. A version mismatch
  never silently falls back to a default layout.

### Matrix

| Contract | Key / type | Introduced | Upgrade-safe? | Migration required? | Fail-closed behavior on mismatch |
| --- | --- | --- | --- | --- | --- |
| Market | `StorageKey::Market(u32)` | v1 | Yes (append-only fields) | No for additive fields; yes if a field is removed/retyped | `UpgradeRequired` on read; writes rejected |
| Market | `StorageKey::Position(u32, Address)` | v1 | Yes | No | `UpgradeRequired` on read; writes rejected |
| Market | `StorageKey::Admin` | v1 | Yes | No | `UpgradeRequired` on read; writes rejected |
| Market | `StorageKey::MarketCounter` | v1 | Yes | No | `UpgradeRequired` on read; writes rejected |
| Market | `StorageKey::OracleAdapters` | v2 | Yes (additive flag) | No | `UpgradeRequired` until v2 migration runs |
| Treasury | `DataKey::StorageVersion` | v1 | Yes | No | `TreasuryError::UpgradeRequired` on read; writes rejected |
| Treasury | `DataKey::Market` / fee routes | v1 | Yes | No | `TreasuryError::UpgradeRequired` on read; writes rejected |
| Resolution | `DataKey::StorageVersion` | v1 | Yes | No | `ResolutionError::UpgradeRequired` on read; writes rejected |
| Resolution | `DataKey::Market` | v1 | Yes | No | `ResolutionError::UpgradeRequired` on read; writes rejected |
| Outcome Token | `DataKey::StorageVersion` | v1 | Yes | No | `OutcomeTokenError::UpgradeRequired` on read; writes rejected |
| Outcome Token | `DataKey::Market` | v1 | Yes | No | `OutcomeTokenError::UpgradeRequired` on read; writes rejected |

### Invariants

1. **Append-only keys.** New storage keys are added as new enum variants; an
   existing variant's name, discriminant, and value type are never changed in
   place. Renaming or retyping a key is a breaking change and requires a
   migration that copies old → new and bumps `StorageVersion`.
2. **Version is written once.** `initialize` writes the current
   `StorageVersion`; only a migration entrypoint may advance it. No other
   code path writes the version key.
3. **Fail-closed reads and writes.** Any read or write performed while
   `stored != expected` returns the contract's `UpgradeRequired` (or
   `IncompatibleStorageVersion`) error. There is no default-value fallback for
   money-path keys (balances, positions, fee routes, market state).
4. **Migration is idempotent.** A migration entrypoint checks the stored
   version first and is a no-op if it has already run, so a replayed or
   concurrent migration call cannot double-apply.
5. **Cross-contract version coordination.** Market, Treasury, Resolution, and
   Outcome Token must be upgraded in the order in [Deploy order](#deploy-order)
   so that no contract reads a counterpart's storage under a version it does
   not understand. A contract whose counterpart reports a newer version fails
   closed rather than proceeding.
6. **No secrets in storage or logs.** Storage keys and migration logs never
   contain private keys, seeds, or admin credentials; only addresses, IDs,
   versions, and error codes are emitted.

### Adding a new storage version

1. Add the new `StorageKey` / `DataKey` variant (append-only).
2. Bump the code's expected `StorageVersion`.
3. Add a migration entrypoint that copies old → new and advances the version;
   make it idempotent (invariant 4).
4. Update this matrix and the per-contract guides in the same PR.
5. Land behind the upgrade feature flag / kill-switch described in
   [Rollback](#rollback) if the change touches a money path.

## Deployment Checklist

Before enabling oracle adapters in production:

- [ ] All four contracts compiled and verified
- [ ] Resolution contract has ≥1 registered adapter (Reflector or Pyth)
- [ ] Market contract has `enable_oracle_adapters()` marked as callable only by authorized account
- [ ] Testnet deployment successful with adapter-to-market resolution flow
- [ ] Mainnet rehearsal: deploy in order, verify resolution works
- [ ] Audit sign-off on upgrade procedure and fail-closed guarantees
- [ ] Storage compatibility matrix above matches the deployed storage layout

## Rollback Procedure

If oracle adapters fail to activate correctly:

1. **Do NOT upgrade market contract alone** — this will disable Ed25519
2. Resolution contract upgrade can be rolled back independently
3. Market contract can be redeployed with `OracleAdapters` key removed from storage
4. Outcome token and treasury are not affected

### Partial Crate Rollback Guide

A **partial crate rollback** reverts one or more of the four crates to a
previously pinned WASM hash while the others keep running the newer code.
This is the common recovery path when a single crate's upgrade misbehaves
(e.g. Resolution registers a bad adapter) and a full four-crate rollback is
unnecessary. Partial rollback is **only** safe when the invariants below hold;
otherwise fall back to the full ordered rollback in
[`rollback.sh`](rollback.sh) and the market storage guide.

#### Invariants (must hold before and after a partial rollback)

1. **Storage version compatibility.** The rolled-back crate's on-chain
   storage version must be readable by the code you are reverting to. Never
   roll a crate back across a storage-version bump unless a dual-read path
   exists (see [Dual-read migration](#dual-read-migration-for-the-next-storage-bump)).
   A version mismatch must surface as `ContractError::UpgradeRequired` /
   `TreasuryError::UpgradeRequired` — never a silent misread.
2. **Settlement idempotency.** Rollback entrypoints are idempotent and
   replay-safe: re-issuing a rollback for an already-reverted crate is a
   no-op that returns the same terminal result, keyed by a stable
   `rollback_id`. Concurrent or replayed requests must not double-apply.
3. **Source of truth.** Balances, swaps, and admin state remain owned by the
   server/contract. A partial rollback must not introduce a second writer for
   any of these; the rolled-back crate continues to read the same
   authoritative storage keys.
4. **Wiring integrity.** Cross-contract addresses registered in step 5 of
   [Deploy order](#deploy-order) must still resolve after the rollback. If a
   rolled-back crate changes an address it previously published, re-wire it
   before re-enabling traffic.

#### Fail-closed behavior

- **Dependency outage (RPC/DB/Redis).** If any dependency needed to verify
  the target WASM hash, storage version, or wiring is unreachable, the
  rollback **fails closed**: no write is applied and the crate stays on its
  current code. Reads may degrade, but writes never proceed on unverified
  state.
- **Privileged surfaces.** Rollback entrypoints are deny-by-default. Only an
  authorized admin role may invoke them; untrusted clients cannot bypass
  rollback policy. Auth expiry or a wrong role returns a stable
  `Unauthorized` error and applies no state change.
- **Adversarial input.** Malformed `rollback_id`, unknown crate id, or a
  target hash that does not match a pinned deployment record is rejected
  before any write.

#### Idempotency, replay, and concurrency

- Every rollback request carries a client-supplied `rollback_id` and a
  correlation id echoed in logs/metrics (no secrets).
- Replaying a completed `rollback_id` returns the recorded terminal result
  rather than re-applying.
- Concurrent requests for the same crate serialize on the crate's rollback
  lock; the loser observes the winner's terminal result.

#### Authz and failure modes

- **Role:** admin-only. Non-admin callers are rejected with `Unauthorized`.
- **Expiry:** an expired admin session is treated as unauthorized; re-auth
  is required before retrying.
- **Wrong role:** rejected without leaking whether the crate exists.

#### Testnet vs mainnet

Partial rollback is rehearsed on testnet first. Contract IDs and pinned WASM
hashes differ between testnet and mainnet (`deployments/testnet.json` vs the
mainnet record); never reuse a testnet hash on mainnet. Mainnet partial
rollback requires the readiness checklist and audit sign-off above.

---

## References

- Market contract oracle module: [contracts/market/src/oracle.rs](../../contracts/market/src/oracle.rs)
- Storage versioning: [contracts/market/src/storage.rs](../../contracts/market/src/storage.rs)
- Storage migration guide: [contracts/market/STORAGE_MIGRATION_GUIDE.md](../../contracts/market/STORAGE_MIGRATION_GUIDE.md)
- Storage version CI check: [contracts/market/STORAGE_VERSION_CI_CHECK.md](../../contracts/market/STORAGE_VERSION_CI_CHECK.md)
- Migration notes: [MIGRATION.md](../../MIGRATION.md)
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

## Storage version compatibility matrix

The authoritative, per-key matrix (which keys are upgrade-safe, which need a
migration, and the fail-closed behavior on version mismatch) lives in the
[Storage Compatibility Matrix](#storage-compatibility-matrix) section above.
It is kept in sync with
[`contracts/market/STORAGE_MIGRATION_GUIDE.md`](../../contracts/market/STORAGE_MIGRATION_GUIDE.md),
[`contracts/market/STORAGE_VERSION_CI_CHECK.md`](../../contracts/market/STORAGE_VERSION_CI_CHECK.md),
and [`MIGRATION.md`](../../MIGRATION.md).

Summary of the invariants enforced by the matrix:

- Storage keys are **append-only**; renaming or retyping a key is a breaking
  change that requires a migration.
- `StorageVersion` is written once on `initialize` and advanced only by an
  idempotent migration entrypoint.
- Reads and writes with `stored != expected` **fail closed**
  (`UpgradeRequired` / `IncompatibleStorageVersion`); there is no default-value
  fallback for money-path keys.
- The four contracts must be upgraded in [Deploy order](#deploy-order) so no
  contract reads a counterpart's storage under an unknown version.

## Dual-read migration for the next storage bump

When a future version adds a key (e.g. `OracleAdaptersV2`), the migration
follows the append-only + idempotent pattern from the matrix:

1. Add the new key variant (append-only).
2. Bump the expected `StorageVersion`.
3. Add an idempotent migration entrypoint that copies old → new and advances
   the version; a replayed or concurrent call is a no-op.
4. Update the matrix and the per-contract guides in the same PR.
5. Land behind the upgrade feature flag / kill-switch if it touches a money
   path, and document rollback in the PR.

## Running the dry-run

/* … truncated 4968 chars — edit only what you need near the top … */
