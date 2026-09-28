# Storage Migration Guide

> **Comprehensive guide for handling storage version bumps in the Vatix Market Contract**

## Table of Contents

1. [Overview](#overview)
2. [Reviewer Checklist: StorageKey Table Drift](#reviewer-checklist-storagekey-table-drift)
3. [When to Bump Storage Version](#when-to-bump-storage-version)
4. [Migration Procedures](#migration-procedures)
5. [Testing Migrations](#testing-migrations)
6. [Rollback and Recovery](#rollback-and-recovery)
7. [Partial Crate Rollback Guide](#partial-crate-rollback-guide)
8. [Common Pitfalls](#common-pitfalls)
9. [Version History](#version-history)

---

## Overview

### What is Storage Versioning?

The Vatix Market Contract uses a storage versioning mechanism to ensure data integrity across contract upgrades. The `STORAGE_VERSION` constant in `src/storage.rs` acts as a compatibility lock:

```rust
pub const STORAGE_VERSION: u32 = 4;
```

> **CI check:** `contracts/market/src/storage.rs` contains a test
> (`test_storage_version_documented_in_migration_guide`) that fails if
> `STORAGE_VERSION` doesn't have a matching `### Version {N}` heading in the
> [Version History](#version-history) section below. Bumping the constant
> without touching this file breaks the build — see that test's doc comment
> for the checklist.

Every storage operation calls `assert_version()` to verify the on-chain version matches the code version. Mismatches return `ContractError::UpgradeRequired`, preventing operations on incompatible data.

### Why Storage Versioning Matters

**Without versioning:**
- New contract code could misinterpret old storage layouts
- Field additions/removals could cause silent data corruption
- Type changes could lead to deserialization errors
- No mechanism to detect incompatible deployments

**With versioning:**
- Explicit compatibility checking
- Safe contract upgrades
- Clear migration paths
- Protection against accidental downgrades

---

## Reviewer Checklist: StorageKey Table Drift

The contract has two independent descriptions of storage that must always
agree:

1. The **`StorageKey` enum** in `src/storage.rs` — the actual, compiled
   source of truth for what can be written to storage.
2. The **`## Storage layout` doc table** at the top of `src/lib.rs` — a
   human-readable summary intended for reviewers and integrators.

Nothing in the compiler enforces that these two stay in sync — the table is
plain doc-comment prose, so it silently drifts whenever a `StorageKey`
variant is added, removed, or renamed without a matching table edit. Check
this on every PR that touches `src/storage.rs` or adds/removes a stored
type:

- [ ] **List every `StorageKey` variant.** Open `src/storage.rs` and read
      the full `pub enum StorageKey { ... }` block.
- [ ] **List every table row.** Open the `## Storage layout` table near the
      top of `src/lib.rs` (in the crate-level `//!` doc comment).
- [ ] **Diff the two lists by hand.** Every enum variant must have exactly
      one corresponding table row (and vice versa — a table row for a key
      that was removed from the enum is just as much drift as a missing
      row).
- [ ] **Check the `Type` and `Description` columns**, not just variant
      names — a field-type change on a variant (e.g. `Address` -> `Vec<Address>`)
      should also update the table's `Type` column.
- [ ] **Cross-check against `STORAGE_VERSION` bumps** — see
      [When to Bump Storage Version](#when-to-bump-storage-version). Most
      changes that require a version bump also change what belongs in this
      table.

A quick local spot-check (not a substitute for reading both lists, but a
useful smoke test) — count that both files mention the same number of
variant identifiers:

```bash
# Enum variant names in storage.rs (rough count, ignores tuple payloads)
grep -oE '^\s{4}[A-Z][A-Za-z]+' contracts/market/src/storage.rs | sort -u

# Row labels in the lib.rs doc table
grep -oE '\| `[A-Za-z]+' contracts/market/src/lib.rs | sort -u
```

**Known example this checklist caught:** as of this writing, `Paused`,
`AdapterEnabled(AdapterType)`, and `DepositLock` exist in the `StorageKey`
enum but were missing from the `lib.rs` table — now fixed alongside this
checklist. Treat any future mismatch the same way: fix the table in the
same PR that changes the enum, don't defer it.

---

## When to Bump Storage Version

### Always Bump When

✅ **Adding new fields to stored types**
```rust
// OLD
pub struct Position {
    market_id: u32,
    user: Address,
    yes_shares: i128,
    no_shares: i128,
}

// NEW - Requires version bump
pub struct Position {
    market_id: u32,
    user: Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128, // NEW FIELD
}
```

✅ **Removing fields from stored types**
```rust
// Removing any field requires version bump
```

✅ **Changing field types**
```rust
// OLD
pub struct Market {
    end_time: u64,
}

// NEW - Requires version bump
pub struct Market {
    end_time: i64, // Type changed
}
```

✅ **Renaming fields (semantic change)**
```rust
// OLD
pub struct Position {
    collateral: i128,
}

// NEW - Requires version bump
pub struct Position {
    total_deposited: i128, // Renamed/semantic change
}
```

✅ **Adding new storage keys**
```rust
// OLD
pub enum StorageKey {
    Market(u32),
    Position(u32, Address),
}

// NEW - Requires version bump
pub enum StorageKey {
    Market(u32),
    Position(u32, Address),
    Treasury, // NEW KEY
}
```

✅ **Changing how existing fields are calculated or used**
```rust
// Example: locked_collateral now derived from shares, not deposits
// Even if the type doesn't change, the semantics do
```

### Safe to Skip

❌ **Adding new functions that don't touch storage**
- Pure computation functions
- View functions that only read existing data

❌ **Fixing bugs that don't change data layout**
- Logic fixes that maintain existing storage semantics
- Event emission changes
- Error message updates

❌ **Documentation changes**
- Comment updates
- Inline documentation

---

## Migration Procedures

### For Testnet Deployments

#### 1. Prepare the Migration

**Step 1.1: Increment the version**
```rust
// src/storage.rs
pub const STORAGE_VERSION: u32 = 4; // Incremented from 3
```

**Step 1.2: Document the change**

Create or update `MIGRATION.md` with:
- What changed in the storage layout
- Why the change was necessary
- Impact on existing data
- Migration steps for data

Example entry:
```markdown
## Version 3 → 4: Added Treasury Integration

### What Changed
- Added `Treasury` storage key
- Added `FeeRateBps` storage key
- Market contract can now route fees to treasury

### Data Migration
No existing data affected. New fields are optional and default to None/0.

### Migration Steps
1. Increment `STORAGE_VERSION` to 4
2. Redeploy contract
3. Call `initialize(admin)` on new deployment
4. Configure treasury with `set_treasury(address)`
```

**Step 1.3: Update tests**

Ensure tests in `src/storage.rs` verify version checking:
```rust
#[test]
fn test_wrong_version_returns_upgrade_required() {
    let env = Env::default();
    let contract_id = env.register(MarketContract, ());
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&StorageKey::StorageVersion, &0u32);
        assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
    });
}
```

#### 2. Build and Deploy

**Step 2.1: Build the new WASM**
```bash
cd contracts/market
stellar contract build
# or
make build
```

**Step 2.2: Deploy to testnet**
```bash
# Set your testnet credentials
export TESTNET_SECRET_KEY="S..."

# Deploy the new WASM
stellar contract deploy \
    --wasm target/wasm32v1-none/release/vatix_market_contract.wasm \
    --source $TESTNET_SECRET_KEY \
    --network testnet

# Output: New contract ID
# CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
```

**Step 2.3: Initialize the new deployment**
```bash
# Call initialize with admin address
stellar contract invoke \
    --id CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC \
    --source $TESTNET_SECRET_KEY \
    --network testnet \
    -- initialize \
    --admin GADMIN...
```

#### 3. Verify the Migration

```bash
# Check storage version
stellar contract invoke \
    --id CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC \
    --source $TESTNET_SECRET_KEY \
    --network testnet \
    -- get_storage_version

# Expected output: 4
```

### For Mainnet Deployments

> ⚠️ **Mainnet deployments require additional safeguards**

1. **Complete testnet validation first** — run the full migration on testnet and observe for at least 48 hours
2. **Prepare rollback plan** — see [Rollback and Recovery](#rollback-and-recovery) and [Partial Crate Rollback Guide](#partial-crate-rollback-guide)
3. **Schedule maintenance window** — coordinate with stakeholders
4. **Deploy during low-activity period** — minimize user impact
5. **Monitor closely** — watch for errors, unusual patterns

---

## Testing Migrations

### Unit Tests

Test version checking and migration logic:

```rust
#[test]
fn test_version_mismatch_fails_closed() {
    let env = Env::default();
    let contract_id = env.register(MarketContract, ());
    env.as_contract(&contract_id, || {
        // Set wrong version
        env.storage().persistent().set(&StorageKey::StorageVersion, &3u32);
        
        // All operations should fail
        let result = some_storage_operation(&env);
        assert_eq!(result, Err(ContractError::UpgradeRequired));
    });
}
```

### Integration Tests

Test the full migration path:

```rust
#[test]
fn test_full_migration_path() {
    // 1. Deploy old version
    // 2. Populate with data
    // 3. Deploy new version
    // 4. Verify data integrity
    // 5. Verify new functionality works
}
```

### Testnet Validation Checklist

- [ ] Deploy new version to testnet
- [ ] Verify storage version is correct
- [ ] Test all critical operations (create market, trade, settle)
- [ ] Verify existing data is readable
- [ ] Check event emissions
- [ ] Monitor for 48 hours
- [ ] Document any issues

---

## Rollback and Recovery

### When to Rollback

Consider rollback when:
- Critical bugs discovered in new version
- Data corruption detected
- Performance degradation
- Security vulnerability found

### Rollback Strategy

> **Important:** Stellar smart contracts are immutable once deployed. "Rollback" means deploying the previous version's WASM to a new contract ID, not reverting an existing deployment.

#### Full Rollback

1. **Stop new operations** — pause the contract if possible
2. **Deploy previous version** — use the last known-good WASM
3. **Migrate data back** — if storage layout changed, migrate data
4. **Update integrations** — point clients to new contract ID
5. **Communicate** — notify users of the change

#### Partial Rollback

For partial rollbacks (rolling back specific functionality while keeping other changes), see the dedicated [Partial Crate Rollback Guide](#partial-crate-rollback-guide) below.

### Recovery Procedures

If data corruption is detected:

1. **Isolate** — stop all writes immediately
2. **Assess** — determine scope of corruption
3. **Snapshot** — export current state for analysis
4. **Restore** — deploy from last known-good state
5. **Verify** — confirm data integrity before resuming

---

## Partial Crate Rollback Guide

> **Scope:** This section defines the procedure for rolling back a *subset* of
> the `vatix-contract` crate (e.g. a single module or entrypoint) without
> reverting the entire deployment. It complements
> [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](../../scripts/upgrade/UPGRADE_PLAYBOOK.md),
> which covers full-crate upgrades. Read both before executing a partial
> rollback on any network.

### Invariants (must hold before, during, and after rollback)

A partial rollback is only valid if **all** of the following invariants are
preserved. If any invariant cannot be guaranteed, abort and escalate to a
full rollback.

1. **Storage version is monotonic and authoritative.**
   `STORAGE_VERSION` in `src/storage.rs` is the single source of truth for
   layout compatibility. A partial rollback **must not** lower
   `STORAGE_VERSION`. If the rolled-back module reads a layout written by a
   newer version, it must fail closed with `ContractError::UpgradeRequired`
   rather than reinterpret bytes.
2. **Settlement idempotency is preserved.**
   Settlement entrypoints remain idempotent: replaying a settlement for an
   already-settled market returns the same terminal result and never
   double-credits balances. Rollback must not reset settlement markers
   (`Settled`, payout records) or clear idempotency keys.
3. **Balances, swaps, and admin remain server/contract-sourced.**
   The contract is the source of truth for balances, swap state, and admin
   authority. A partial rollback must not introduce any client-trusted
   balance, swap, or admin value, and must not weaken `require_auth` on
   privileged entrypoints.
4. **No privileged surface is widened.**
   Rollback is deny-by-default: any entrypoint not explicitly re-enabled by
   the rollback plan stays disabled. New privileged surfaces introduced by
   the rolled-back change are removed, not merely hidden.

### Fail-closed behavior on dependency outage

Partial rollback must never trade availability for correctness on the money
path. When a dependency is unavailable, writes fail closed:

| Dependency | Failure mode | Required behavior |
| --- | --- | --- |
| RPC / network | Timeout or error | Abort the write; do not retry blindly. Surface a stable error to the caller. |
| DB / indexer | Unavailable or stale | Reject writes that depend on indexed state; reads may serve last-known-good with an explicit staleness marker. |
| Redis / cache | Unavailable | Bypass cache and read from the contract; never serve cached balances as authoritative. |

- **Writes fail closed.** If the contract cannot confirm the current
  storage version or settlement state, the write is rejected.
- **Reads may degrade, never lie.** Degraded reads must be clearly marked
  and must not be used to authorize a write.
- **No silent fallback.** A failed dependency must produce an observable
  error, not a default value.

### Idempotency, replay, and concurrency

Rollback entrypoints are themselves money-path operations and must be safe
under retries and concurrent callers:

- **Idempotency key.** Every rollback request carries a caller-supplied
  `rollback_id` (or equivalent correlation id). Re-submitting the same
  `rollback_id` returns the original result and performs no additional
  state change.
- **Replay protection.** A rollback request that has already been applied
  is rejected with a stable error code (see below) rather than re-applied.
- **Concurrency.** Two concurrent rollback requests for the same scope must
  not both apply. The first to acquire the rollback lock wins; the second
  observes the completed state and returns the idempotent result.
- **Correlation ids.** Every rollback log line and error response includes
  the `rollback_id` so operators can trace a request end-to-end without
  exposing secrets.

### Stable error codes

Rollback entrypoints return stable, documented error codes so callers and
runbooks can branch deterministically:

| Code | Meaning |
| --- | --- |
| `RollbackUnauthorized` | Caller lacks the required role, or auth has expired. |
| `RollbackAlreadyApplied` | The `rollback_id` was already applied (replay). |
| `RollbackInProgress` | Another rollback holds the lock for this scope. |
| `RollbackVersionConflict` | Storage version is incompatible; fail closed. |
| `RollbackDependencyUnavailable` | A required dependency (RPC/DB/Redis) is down; write rejected. |

### Authorization requirements

- **Deny-by-default.** Only the admin role (or an explicitly delegated
  rollback role) may invoke rollback entrypoints. Untrusted clients cannot
  bypass rollback policy by calling the underlying module directly.
- **Auth expiry.** An expired or missing auth token is treated as
  unauthorized (`RollbackUnauthorized`); the request is rejected, never
  downgraded to a read.
- **Wrong role.** A caller with a valid but insufficient role is rejected
  with `RollbackUnauthorized` and the attempt is logged with the
  `rollback_id`.
- **No privilege escalation via rollback.** Rollback must not grant any
  role or capability that the caller did not already hold.

### Observability

- Emit a metric for every rollback attempt, tagged by outcome
  (`applied`, `rejected`, `in_progress`) and scope.
- Log the `rollback_id`, scope, and outcome. **Never** log secrets, keys,
  or full addresses beyond what is already public.
- Alert on any `RollbackVersionConflict` or
  `RollbackDependencyUnavailable` — these indicate a fail-closed event on
  the money path.

### Feature flag / kill switch

Any partial rollback that affects the money path or mainnet must land
behind a feature flag or kill switch:

- The flag defaults to **off**; enabling it is an explicit, audited action.
- Flipping the flag off must restore the pre-rollback behavior without a
  redeploy where the platform allows it.
- The PR that introduces the rollback must document the flag name, its
  default, and the exact steps to disable it.

### Rollback procedure (step-by-step)

1. **Confirm scope.** Identify the exact module/entrypoint to roll back and
   the `rollback_id` for this operation.
2. **Verify invariants.** Re-check the four invariants above against the
   current on-chain state. Abort on any violation.
3. **Check authz.** Confirm the caller holds the admin/rollback role and
   that auth has not expired.
4. **Acquire the rollback lock.** If another rollback is in progress for
   this scope, stop (`RollbackInProgress`).
5. **Apply behind the flag.** Enable the flag, apply the rollback, and
   confirm the storage version is unchanged.
6. **Verify.** Re-run the settlement idempotency and balance checks. Confirm
   no privileged surface was widened.
7. **Record.** Persist the `rollback_id` as applied so replays return
   `RollbackAlreadyApplied`.
8. **Communicate.** Update the runbook and notify Stellar Wave contributors
   via the PR description and the cross-links in
   [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](../../scripts/upgrade/UPGRADE_PLAYBOOK.md).

### Testnet vs mainnet

- **Testnet:** run the full procedure end-to-end, including the flag flip
  and a deliberate replay to confirm `RollbackAlreadyApplied`.
- **Mainnet:** require the readiness checklist from the UPGRADE_PLAYBOOK,
  a scheduled window, and a documented rollback-of-the-rollback. Do not
  perform an irreversible mainnet partial rollback without that checklist.
- **Address drift:** verify contract IDs and admin addresses against the
  network before applying; a rollback against the wrong network is a
  fail-closed abort, not a best-effort attempt.

---

## Common Pitfalls

### 1. Forgetting to Bump Version

**Problem:** Changed storage layout but didn't increment `STORAGE_VERSION`.

**Impact:** Old code may read new data incorrectly, causing corruption.

**Solution:** Always bump version when changing storage layout.

### 2. Not Testing Migration Path

**Problem:** Migration works in isolation but fails with real data.

**Impact:** Production deployment fails or corrupts data.

**Solution:** Test with realistic data volumes and patterns.

### 3. Missing Rollback Plan

**Problem:** No plan for reverting if migration fails.

**Impact:** Extended downtime, data loss.

**Solution:** Always have a rollback plan. See [Rollback and Recovery](#rollback-and-recovery) and [Partial Crate Rollback Guide](#partial-crate-rollback-guide).

### 4. Incomplete Data Migration

**Problem:** Some data migrated, some not.

**Impact:** Inconsistent state, failed operations.

**Solution:** Migrate all data atomically or in a verifiable sequence.

### 5. Ignoring Storage Key Drift

**Problem:** `StorageKey` enum and `lib.rs` doc table diverge.

**Impact:** Reviewer confusion, missed keys, integration bugs.

**Solution:** Run the [Reviewer Checklist](#reviewer-checklist-storagekey-table-drift) on every storage PR.

---

## Version History

### Version 4 (Current)

**Changes:**
- Added `Treasury` storage key
- Added `FeeRateBps` storage key
- Treasury integration for fee routing

**Migration from v3:**
- No data migration required
- New keys are optional
- Redeploy and initialize

### Version 3

**Changes:**
- Added `DepositLock` storage key
- Added `Paused` storage key
- Added `AdapterEnabled(AdapterType)` storage key

**Migration from v2:**
- No data migration required
- New keys default to disabled/unpaused

### Version 2

**Changes:**
- Added position tracking
- Added market settlement state

**Migration from v1:**
- Positions migrated from v1 format
- Settlement state initialized

### Version 1

**Initial release:**
- Basic market creation
- Position tracking
- Settlement

---

## Additional Resources

- [Upgrade Playbook](../../scripts/upgrade/UPGRADE_PLAYBOOK.md)
- [Security Policy](../../SECURITY.md)
- [Contributing Guide](../../CONTRIBUTING.md)
- [Stellar Contract Documentation](https://developers.stellar.org/docs/smart-contracts)

---

**Last Updated:** See git history for this file.
