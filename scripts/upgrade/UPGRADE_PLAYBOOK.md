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

## Upgrade order invariants

These invariants are enforced by [`upgrade.sh`](upgrade.sh) and must hold for
every cross-contract upgrade. A violation aborts the run before any
mainnet-affecting transaction is submitted.

1. **Dependencies before dependents.** A contract is only upgraded after
   every contract it calls into has been upgraded and re-wired. Concretely:
   Market → Treasury, Market → Outcome Token, Market → Resolution, and
   Resolution → Market. Upgrading a dependent first would let it call a
   counterpart whose storage layout it does not yet understand.
2. **Storage version compatibility.** The new WASM's storage version must be
   `>=` the on-chain version and `<=` the version the current code can read
   (see the [compatibility matrix](#storage-version-compatibility-matrix)).
   Downgrades and skipped versions are rejected.
3. **Admin/authz preserved.** The admin address recorded on-chain must be
   unchanged across the upgrade, and every privileged entrypoint
   (`set_*`, `enable_oracle_adapters`, `upgrade`) must remain
   admin-gated. The script refuses to proceed if the new WASM exposes an
   ungated privileged entrypoint.
4. **Address stability.** Contract IDs must match the recorded deployment
   for the target network; address drift between testnet and mainnet aborts
   the run.
5. **Fail-closed on missing preconditions.** Missing env vars, an unexpected
   network passphrase, or a failed preflight check abort the run rather than
   proceeding with defaults.

## WASM hash pinning

Every upgrade pins the expected WASM hash for each contract in
`deployments/<network>.json`. `upgrade.sh` recomputes the hash of the built
artifact and aborts on mismatch, so a stale or tampered build cannot be
submitted.

## Storage version compatibility matrix

| Contract       | Current | Min readable | Notes                                  |
| -------------- | ------- | ------------ | -------------------------------------- |
| Market         | v2      | v1           | Dual-read for `OracleAdapters`         |
| Treasury       | v1      | v1           | —                                      |
| Resolution     | v1      | v1           | —                                      |
| Outcome Token  | v1      | v1           | —                                      |

## Dual-read migration for the next storage bump

When bumping a storage version, land the reader first: add the new key,
read new-then-old, and only remove the old read path in a follow-up release
after every deployment has migrated. This keeps a rolling upgrade safe.

## Running the dry-run

```
# Fail-closed: aborts on missing env, wrong network, or failed preflight.
NETWORK=testnet \
ADMIN_SECRET_KEY=... \
MARKET_ID=... TREASURY_ID=... RESOLUTION_ID=... OUTCOME_TOKEN_ID=... \
  ./scripts/upgrade/upgrade.sh --dry-run
```

Drop `--dry-run` only after the staging checklist below passes. The script
prints a correlation id per step and emits structured logs (no secrets) so
runs can be traced in CI and ops dashboards.

## Staging dry-run checklist

- [ ] `upgrade.sh --dry-run` completes with all preflight checks green.
- [ ] WASM hashes match the pinned values in `deployments/<network>.json`.
- [ ] Storage versions satisfy the compatibility matrix.
- [ ] Admin address unchanged; privileged entrypoints still gated.
- [ ] Resolution → Market and Market → Treasury callbacks verified on testnet.
- [ ] Rollback rehearsed (see below) before touching mainnet.

## CI enforcement

[`scripts/upgrade/check-upgrade-order.sh`](check-upgrade-order.sh) runs the
same preflight checks in CI and fails the build if the documented order or
invariants are violated. This keeps the playbook and the script in sync.

## Rollback

If an upgrade fails mid-sequence:

1. Stop — do not upgrade any further contract.
2. Run [`rollback.sh`](rollback.sh) to restore the previous WASM hashes and
   contract IDs recorded in `deployments/<network>.json`.
3. Re-verify wiring (step 5 above) and re-run the staging checklist.
4. Document the incident and the correlation ids from the failed run.
