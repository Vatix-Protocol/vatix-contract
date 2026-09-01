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

## Files touched

- `contracts/market/STORAGE_MIGRATION_GUIDE.md` — added Version 4 entry,
  fixed stale version references.
- `contracts/market/src/storage.rs` — new guide-linkage test and checklist
  doc comment.
