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
treasury is registered) rather than silently using a stale address — but a
**partial** re-wire (e.g. `set_resolution_contract` updated to point at a
new Resolution deployment while Resolution still points at the *old* Market
address) is exactly the "partial upgrade bricks finalize" failure mode this
playbook exists to catch. Always re-wire both sides of a relationship in the
same maintenance window.

**`Market::void_market` (Issue #708).** The resolution contract's
`void_market` dispute path invokes `MarketContract::void_market(caller,
market_id)` as its final step. That market entrypoint authorizes the call by
requiring `caller == storage::get_resolution_contract(env)` and **fails
closed with `Unauthorized`** when no resolution contract is registered or the
caller is anyone else (the admin included). So the Market↔Resolution wiring
in step 5 must be complete before a `void_market` can succeed, and after any
Resolution redeployment `Market::set_resolution_contract` must be pointed at
the new address in the same window — otherwise a stuck disputed market cannot
be voided.

## WASM hash pinning

[`scripts/verify-wasm-hash.sh`](../verify-wasm-hash.sh) computes the
SHA-256 hash of a single contract's build. This playbook adds
[`expected-hashes.json`](expected-hashes.json) as the **pinned** reference
for all four contracts at once, and
[`check-upgrade.sh`](check-upgrade.sh) as the script that builds every
contract and fails if a pinned hash doesn't match what was just built.

### Fail-closed semantics (pinned vs. unpinned)

`check-upgrade.sh` treats each contract's `expectedSha256` as a **pin**:

- **Unpinned** (`expectedSha256` is `null` or absent): the contract is not
  yet pinned, so the check is skipped with a warning and does **not** fail
  the run. This is the placeholder convention already used in
  `deployments/testnet.json`.
- **Pinned** (`expectedSha256` is a non-empty string): the freshly built
  artifact **must** match exactly. Any drift — a mismatched hash, or an
  empty/whitespace-only value that was meant to be a pin — exits non-zero
  and fails the dry-run. There is no silent fallback: a pinned contract
  that cannot be verified is a hard failure, not a warning.

This is deliberate. Once you've picked the exact commit/build you intend to
roll out, fill in the real hash (`bash scripts/verify-wasm-hash.sh
<contract-dir>`) and commit `expected-hashes.json`. From that point, any
accidental rebuild drift (toolchain change, uncommitted local edit, wrong
branch) fails the dry-run instead of silently shipping a different artifact
than the one that was reviewed.

### Pinning and unpinning a contract

- **To pin:** run `bash scripts/verify-wasm-hash.sh <contract-dir>` for the
  contract, copy the resulting SHA-256 into that contract's
  `expectedSha256` in `expected-hashes.json`, and commit it in the same PR
  as the build you intend to ship.
- **To unpin (intentionally):** set `expectedSha256` back to `null` (not an
  empty string) and commit with a rationale. `null` is the explicit
  "not yet pinned" signal; an empty string is treated as a broken pin and
  fails closed.

## Storage version compatibility matrix

[`version-matrix.json`](version-matrix.json) is the machine-checked source
of truth for which `STORAGE_VERSION` values are compatible across
contracts. `check-upgrade.sh` Phase A compares it against the
`STORAGE_VERSION` constants compiled into `contracts/market/src/storage.rs`,
`contracts/treasury/src/storage.rs`, `contracts/resolution/src/storage.rs`,
and `contracts/outcome-token/src/storage.rs`, and fails on drift — the same
"drift" failure mode the market contract's own
`test_storage_version_documented_in_migration_guide` test guards against,
extended across contracts.

**Update (Issue #696):** Resolution and Outcome Token now carry their own
`STORAGE_VERSION` constant and `storage::assert_version` guard, same as
market and treasury — they're tracked in the matrix as
`versioningScheme: "storageVersion"` like the other two, currently both at
`storageVersion: 1` (their first versioned layout; no schema shape changed,
only the guard was added). Before this, a partial cross-contract upgrade
that redeployed Resolution or Outcome Token onto a fresh/mismatched storage
layout had no on-chain guard at all — the old `wasmHashOnly` scheme only
caught a build-artifact drift, not a *storage* drift, so a stale deployment
could keep serving `finalize`/`mint`/`burn` calls against a layout the
compiled contract no longer agreed with, silently corrupting state instead
of failing closed with `UpgradeRequired`. `assert_version` now runs at the
top of every state-mutating entry point on both contracts (finalize,
propose, challenge, appeal, arbitrate_uphold_proposer, void_market, and the
address-rotation calls on Resolution; mint, burn, transfer, and the config
setters on Outcome Token), so an unversioned or stale deployment now fails
closed exactly like market and treasury already did. WASM-hash pinning via
`expected-hashes.json` (see above) is unaffected and still applies to all
four contracts regardless of storage-versioning scheme — it catches
build-artifact drift, which is an orthogonal concern to storage-layout
drift.

**Updating the matrix:** whenever you bump `STORAGE_VERSION` on any of the
four contracts, add the new value to `version-matrix.json`'s
`contracts.<name>` entry and append a row to `compatibility` recording the
new `<name>StorageVersion` alongside the others it's compatible with, in
the same PR — exactly like the existing `STORAGE_MIGRATION_GUIDE.md`
"Version History" convention.

## Dual-read migration for the next storage bump

The next time market or treasury's `STORAGE_VERSION` needs to move forward
(e.g. market v4 → v5), reach for the dual-read pattern in
[`DUAL_READ_MIGRATION_TEMPLATE.md`](DUAL_READ_MIGRATION_TEMPLATE.md) instead
of "fresh deployment only" if you want a **live** deployment to serve reads
from both the old and new storage shapes during a compatibility window
before every writer has switched to the new format.

## Running the dry-run

```bash
bash scripts/upgrade/check-upgrade.sh
```

Exit code `0` = pass (including phases skipped because `stellar`/`cargo`
aren't on `PATH` — see the script header), exit code `1` = fail. This is the
single command referenced by the [staging checklist](#staging-dry-run-checklist)
below and by CI.

## Staging dry-run checklist

See [`STAGING_DRY_RUN_CHECKLIST.md`](STAGING_DRY_RUN_CHECKLIST.md) for the
full testnet rehearsal checklist (deploy order, wiring, version-skew
verification, rollback rehearsal).

## CI enforcement

The `upgrade-dry-run` job in
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs
`check-upgrade.sh` on every push/PR with the full toolchain available
(Rust + Stellar CLI), so:

- Storage-version drift between source and `version-matrix.json` fails CI.
- A pinned WASM hash that doesn't match the freshly built artifact fails CI,
  and a pinned contract with an empty/missing hash fails closed rather than
  warning.
- The `UpgradeRequired` regression tests for all four contracts (market,
  treasury, resolution, outcome-token — see #696) run as part of the same
  job, so a change that accidentally removes a version guard (see "Pitfall
  2" in `STORAGE_MIGRATION_GUIDE.md`) fails CI too.

## Rollback

Vatix contracts use a **fresh-deployment** upgrade model (see
`STORAGE_MIGRATION_GUIDE.md` → "Mainnet Migration Options" → "Fresh
Deployment"), not in-place WASM replacement. There is no on-chain "undo" —
rollback means re-pointing downstream consumers (frontend, scripts, indexers)
back at the previous, still-live contract ID.

```bash
# Preview what would change (does not modify any files):
bash scripts/upgrade/rollback.sh HEAD~1

# Apply it (writes the previous deployments/testnet.json over the working copy;
# review and commit the result yourself):
bash scripts/upgrade/rollback.sh HEAD~1 --apply
```

**When state is unrecoverable:** any deposit, trade, or position written
*only* to the new deployment after cutover is not present on the old
deployment — re-pointing the registry does not migrate that data backward.
If the new deployment has already taken writes that matter, this is a data
recovery problem, not a rollback problem — follow "If Migration Fails on
Mainnet" in `STORAGE_MIGRATION_GUIDE.md` instead (communicate, assess
impact, choose fix-forward vs. restore vs. custom recovery script).
