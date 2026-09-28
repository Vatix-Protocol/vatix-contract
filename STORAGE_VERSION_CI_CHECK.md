# Storage Version Bump Checklist Test

## Issue

`STORAGE_VERSION` guards upgrades (mismatches return
`ContractError::UpgradeRequired`), but nothing enforced that
`STORAGE_MIGRATION_GUIDE.md` actually gets updated when the constant is
bumped — the guide had drifted: `STORAGE_VERSION` was already `4` in
`contracts/market/src/storage.rs` (with the change documented inline in that
file's doc comment for v4, adding `StorageKey::AdapterEnabled` for #488), but
`STORAGE_MIGRATION_GUIDE.md`'s "Version History" section still listed
"Version 3 (Current)" with no Version 4 entry at all.

## What this change does

1. **Fixes the drift**: added a `### Version 4 (Current)` section to
   `contracts/market/STORAGE_MIGRATION_GUIDE.md`'s Version History,
   documenting the `AdapterEnabled` storage key addition, and demoted the
   old `### Version 3 (Current)` heading to plain `### Version 3`. Also
   fixed the stale `STORAGE_VERSION: u32 = 3` code snippet in the Overview
   section to `= 4`.

2. **Adds a lightweight CI guard**:
   `test_storage_version_documented_in_migration_guide` in
   `contracts/market/src/storage.rs` reads
   `STORAGE_MIGRATION_GUIDE.md` via `include_str!` at compile time and
   asserts:
   - There is a `### Version {STORAGE_VERSION} (Current)` heading matching
     the constant currently in code.
   - Exactly one `(Current)` marker exists in the file (catches a bump that
     added a new section but forgot to demote the old "(Current)" one,
     which would otherwise let a naive substring check pass vacuously).

   This makes a future `STORAGE_VERSION` bump without touching the guide
   fail `cargo test` immediately, rather than only being caught in review —
   satisfying "fail CI if version constant changes without guide touch"
   without needing a separate git-diff-based CI step.

3. **Confirms existing coverage** for the other acceptance criteria:
   `test_wrong_version_returns_upgrade_required` and
   `test_missing_version_returns_upgrade_required` (already present in
   `contracts/market/src/storage.rs`) already assert `assert_version`
   returns `UpgradeRequired` on mismatch/missing version — no change needed
   there.

## Storage compatibility matrix

The playbook (`scripts/upgrade/UPGRADE_PLAYBOOK.md`) and this checklist must
agree with the actual on-chain layout. The matrix below is the single source
of truth for which storage surfaces are upgrade-safe, which require a
migration, and how incompatible versions fail closed. It is kept consistent
with `contracts/market/STORAGE_MIGRATION_GUIDE.md`,
`contracts/market/MIGRATION.md`, and the `STORAGE_VERSION` constant in
`contracts/market/src/storage.rs`.

| Storage surface | Key / type | Upgrade-safe? | Migration required? | Fail-closed behavior |
| --- | --- | --- | --- | --- |
| Version marker | `StorageKey::Version` (`u32`) | Yes — read first on every entrypoint | No (must match `STORAGE_VERSION`) | `assert_version` returns `ContractError::UpgradeRequired` on mismatch or missing marker; writes are rejected |
| Market config | `StorageKey::Config` (`MarketConfig`) | Yes — append-only fields | Only if a field is added/removed/reordered | `UpgradeRequired` if version gate fails before deserialize |
| Positions | `StorageKey::Position(Address)` (`Position`) | Yes — per-account, additive | Only on type change | `UpgradeRequired`; no partial reads |
| Adapter enablement | `StorageKey::AdapterEnabled` (`bool`) | Yes — added in v4 (#488) | No for v4 (new key, defaults absent) | Absent key treated as disabled (deny-by-default) |
| Admin / roles | `StorageKey::Admin` (`Address`) | Yes — set at init | No | `UpgradeRequired`; authz checks run after version gate |
| Pause / kill-switch | `StorageKey::Paused` (`bool`) | Yes | No | Absent key treated as paused for money paths (fail-closed) |

### Invariants

- **Version gate first.** Every external entrypoint calls `assert_version`
  before reading or writing any other key. A mismatch or missing marker
  returns `UpgradeRequired` and no state is mutated.
- **Additive keys are upgrade-safe.** New `StorageKey` variants (e.g.
  `AdapterEnabled` in v4) do not require a migration; absent keys must be
  interpreted with a deny-by-default default.
- **Type or layout changes require a migration.** Any change to an existing
  key's type, field order, or semantics requires a documented migration in
  `STORAGE_MIGRATION_GUIDE.md` and a `STORAGE_VERSION` bump.
- **Fail-closed on incompatible versions.** Money paths (liquidity, trading,
  settlement) must not proceed when the version gate fails; they return
  `UpgradeRequired` rather than reading stale or unknown layouts.
- **Guide and code stay in lockstep.** The `STORAGE_VERSION` constant, the
  `### Version {N} (Current)` heading in the migration guide, and this matrix
  must all reference the same version; the CI guard above enforces the first
  two.

## Files touched

- `contracts/market/STORAGE_MIGRATION_GUIDE.md` — added Version 4 entry,
  fixed stale version references.
- `contracts/market/src/storage.rs` — new guide-linkage test and checklist
  doc comment.
- `scripts/upgrade/UPGRADE_PLAYBOOK.md` — storage compatibility matrix
  aligned with the layout above.

## Contributor runbook

The drift check is deterministic and dependency-free: it runs as part of the
existing `cargo test` step in `.github/workflows/ci.yml` (no new workflow
step, no new tooling). To bump storage safely:

1. Increment `STORAGE_VERSION` in `contracts/market/src/storage.rs`.
2. Add a `### Version {N} (Current)` section to
   `contracts/market/STORAGE_MIGRATION_GUIDE.md` and demote the previous
   `(Current)` heading to plain `### Version {N-1}`.
3. Document any new `StorageKey` variants and their migration path in the
   new section, and update the storage compatibility matrix in
   `scripts/upgrade/UPGRADE_PLAYBOOK.md` to match.
4. Run `cargo test -p vatix-market` locally; the guide-linkage test fails
   closed if steps 1–3 are inconsistent.

If the check fails in CI, treat it as a hard stop: do not merge a
`STORAGE_VERSION` bump until the guide matches, since a mismatch would leave
on-chain state unreadable after upgrade.
