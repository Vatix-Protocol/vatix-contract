# Market Contract Storage Migration Guide

This document describes the storage layout of the `market` contract, the
versioning scheme used to detect incompatible upgrades, and the migration
procedures required when the layout changes.

> **Cross-references**
> - `scripts/upgrade/UPGRADE_PLAYBOOK.md` — operator playbook and the
>   storage compatibility matrix (source of truth for upgrade decisions).
> - `contracts/market/STORAGE_MIGRATION_GUIDE.md` — step-by-step migration
>   runbook for layout-changing upgrades.
> - `contracts/market/STORAGE_VERSION_CI_CHECK.md` — CI gate that fails the
>   build when the on-chain storage version is bumped without a migration.

## Storage versioning scheme

The contract stores a single `DataKey::StorageVersion(u32)` entry. The value
is the **current** storage schema version. Every upgrade must satisfy:

- `new_version >= old_version` — downgrades are rejected (fail-closed).
- `new_version == old_version` — no migration required; the upgrade is
  considered *upgrade-safe*.
- `new_version > old_version` — a migration entrypoint must exist and be
  invoked before any state-dependent call is served.

If the stored version is **greater** than the version compiled into the
contract, all state-mutating entrypoints must revert with
`Error::IncompatibleStorageVersion` (fail-closed). Read-only entrypoints may
continue to serve only if the layout is provably compatible; otherwise they
must also revert.

## Compatibility matrix

| Storage key / type | Upgrade-safe (no migration) | Requires migration | Fail-closed behavior |
| --- | --- | --- | --- |
| `DataKey::StorageVersion(u32)` | — | Always written on upgrade | Revert `IncompatibleStorageVersion` if stored > compiled |
| `DataKey::Admin` | ✅ | ❌ | Revert `Unauthorized` if missing |
| `DataKey::Config` (append-only fields) | ✅ | ❌ | Revert `InvalidConfig` on decode failure |
| `DataKey::Config` (field removal / reorder) | ❌ | ✅ | Revert `IncompatibleStorageVersion` until migrated |
| `DataKey::Market(u64)` | ✅ | ❌ | Revert `MarketNotFound` if absent |
| `DataKey::Market(u64)` (struct field change) | ❌ | ✅ | Revert `IncompatibleStorageVersion` until migrated |
| `DataKey::Position(Address, u64)` | ✅ | ❌ | Revert `PositionNotFound` if absent |
| `DataKey::Position(Address, u64)` (struct field change) | ❌ | ✅ | Revert `IncompatibleStorageVersion` until migrated |
| `DataKey::Settlement(u64)` | ✅ | ❌ | Revert `SettlementNotFound` if absent |
| `DataKey::Settlement(u64)` (idempotency marker) | ✅ | ❌ | Revert `AlreadySettled` on replay |
| `DataKey::ChallengeBond(u64)` | ✅ | ❌ | Revert `BondLocked` / `BondNotLocked` on invalid state |
| `DataKey::Paused` | ✅ | ❌ | Deny-by-default: mutating calls revert `Paused` |

Legend: ✅ = safe to upgrade without a migration; ❌ = migration required.

### Invariants

1. **Version monotonicity.** `StorageVersion` never decreases. Any attempt to
   write a lower version reverts.
2. **Fail-closed on mismatch.** If `stored_version > compiled_version`, every
   state-mutating entrypoint reverts with `IncompatibleStorageVersion`.
3. **Migration atomicity.** A migration entrypoint must either complete fully
   or revert; partial migrations are not observable.
4. **Idempotent settlement.** `DataKey::Settlement(u64)` is written exactly
   once per market; replays revert `AlreadySettled`.
5. **Bond lock/unlock symmetry.** `DataKey::ChallengeBond(u64)` transitions
   only `None → Locked → None`; any other transition reverts.
6. **No secrets in storage.** Storage entries never contain private keys,
   API tokens, or off-chain credentials.

## Migration procedure

1. Bump `STORAGE_VERSION` in the contract source.
2. Add a `migrate(from: u32, to: u32)` entrypoint guarded by `Admin` authz.
3. Update the compatibility matrix above and the playbook matrix in
   `scripts/upgrade/UPGRADE_PLAYBOOK.md` in the same PR.
4. Ensure `STORAGE_VERSION_CI_CHECK.md` passes (version bump ⇒ migration).
5. Land behind the upgrade feature flag; document rollback in the PR.

## Rollback

If a migration fails post-deploy, the operator must:

- Pause the contract (`DataKey::Paused = true`) to deny-by-default.
- Restore the previous WASM build (same `StorageVersion` as before the bump).
- Re-run the migration only after the root cause is fixed and reviewed.

Rollback never downgrades `StorageVersion`; the restored build must be
compatible with the stored version or the contract stays paused.
