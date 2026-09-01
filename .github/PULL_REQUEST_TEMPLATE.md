## Summary

<!-- What does this PR do? Reference the issue it closes: "Closes #NNN" -->

## Changes

<!-- Brief bullet list of what changed -->

## Testing

<!-- How was this tested? Which test files / commands cover it? -->

---

## Reviewer checklist

### General
- [ ] PR is scoped to one issue
- [ ] New public entrypoints are documented in `docs/AUTH_TABLE.md` and `docs/events-reference.md`
- [ ] `cargo fmt --check` passes for every touched crate
- [ ] `cargo clippy -- -D warnings` passes for every touched crate
- [ ] `cargo check --workspace --all-targets` passes

### Storage changes
If this PR adds, removes, renames, or changes the type/semantics of a `StorageKey` variant:
- [ ] `STORAGE_VERSION` was bumped in `contracts/market/src/storage.rs`
- [ ] A `### Version {N} (Current)` section was added to `contracts/market/STORAGE_MIGRATION_GUIDE.md`
- [ ] `contracts/market/MIGRATION.md` has an entry for this bump
- [ ] `scripts/upgrade/version-matrix.json` `storageVersion` for `market` matches

### Reentrancy / CEI audit (#784)
**Required for every PR that adds or modifies a `TokenClient::transfer` call
(or any other external token / cross-contract call that moves value).**

For each new or modified `token_client.transfer` / `TokenClient::new(...).transfer(...)` call site:

- [ ] All storage state writes that gate re-entry (position balances, settled
  flags, lock fields) run **before** the external transfer call
  (Checks-Effects-Interactions order).
- [ ] No storage write that an attacker could use to re-enter and claim funds
  a second time appears **after** the transfer.
- [ ] A regression test exists (or is added in this PR) that asserts the
  idempotency boundary: a second call after the first succeeds is rejected
  with an appropriate error (`InsufficientCollateral`, `PositionAlreadySettled`,
  etc.), proving state was committed before the transfer.
- [ ] The new call site is documented in `docs/reentrancy-cei-audit.md` with
  its CEI status (`Compliant` or the issue reference for any accepted
  exception).

> **If no `TokenClient::transfer` calls were added or modified**, check this
> box and write "N/A — no new/modified transfer calls":
> - [ ] N/A — no new/modified transfer calls in this PR

### No new instant admin mutators
- [ ] Any new admin-gated mutator that peers use a timelock for also uses
  `FEE_RATE_TIMELOCK_SECONDS` (172 800 s / 48 h), or the deviation is
  explicitly justified in the PR description.
