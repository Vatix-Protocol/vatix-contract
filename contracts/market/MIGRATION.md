# Migrations

## Version 4: Close Market to Deposits Feature

**Release**: June 2026  
**Storage Version**: 4 (bumped from 3)  
**Breaking Change**: YES - Requires reinitialization

### What Changed

Added a new `closed_to_deposits: bool` field to the `Market` struct. This field allows administrators to prevent new collateral deposits while preserving all other functionality (trading, withdrawals, settlement).

### API Addition

```rust
pub fn close_market_to_deposits(
    env: Env,
    admin: Address,
    market_id: u32,
) -> Result<(), ContractError>
```

### Error Code Addition

- **Error 6: `MarketClosedToDeposits`** – Returned when a user attempts to deposit into a market that has been closed to new deposits.

### Event Addition

```rust
MarketClosedToDepositsEvent {
    market_id: u32,
    admin: Address,
    closed_at: u64,
}
```

### Migration Instructions

1. **Increment STORAGE_VERSION**: Changed from 3 to 4 in `storage.rs`
2. **Rebuild**: Run `stellar contract build` to produce new WASM
3. **Redeploy**: Deploy the new WASM to your target network
4. **Reinitialize**: Call `initialize(admin)` on the fresh deployment

### Breaking Changes

- Old deployments (v3) cannot read markets from new deployments (v4)
- Any call to an old v3 contract will return `UpgradeRequired` error
- Existing markets must be recreated after upgrade

### Backward Compatibility

- All newly created markets have `closed_to_deposits = false` (open by default)
- No existing functionality is removed or changed
- Withdrawals and settlement continue to work normally when a market is closed to deposits

### Data Migration Path

**On Testnet**: 
1. Redeploy contract
2. Reinitialize admin
3. Recreate test markets
4. Rerun integration tests

**On Production**: 
- Plan a migration window
- Notify users that markets will briefly become unavailable
- Redeploy, reinitialize, and migrate user positions as needed

---

# Migration: `Position::locked_collateral` is now share-based only (#262)

## What changed

`Position::locked_collateral` is now exclusively a derived, share-based value
maintained by `positions::update_position` (via `calculate_locked_collateral`).
No other code path writes to this field:

- `deposit_collateral` only increments `total_deposited`. It no longer adds
  the deposit amount to `locked_collateral`.
- `withdraw_unused_collateral` reads `Position::locked_collateral` directly
  instead of recomputing a required lock from a hardcoded 50/50 market price.

## Why

Before this change, three code paths computed "locked collateral" three
different ways:

- `deposit_collateral` set `locked_collateral = locked_collateral + amount`,
  so a user who deposited but never bought any shares already showed their
  entire deposit as locked.
- `update_position` correctly set `locked_collateral` from
  `calculate_locked_collateral(yes_shares, no_shares, market_price)`.
- `withdraw_unused_collateral` ignored the stored field entirely and
  recomputed its own lock using a hardcoded `MARKET_PRICE_BPS = 5_000`,
  which diverged from the real trade price whenever shares were bought at
  any price other than 50/50.

## Impact on existing/seeded state and test fixtures

Any `Position` record (on-chain, in a local snapshot, or hand-built in a
test) that was produced by the old `deposit_collateral` and has
`locked_collateral == total_deposited` while `yes_shares == 0 && no_shares
== 0` reflects the old, incorrect accounting. It must be recomputed, not
copied forward:

- The correct value for such a position is `locked_collateral = 0` (no
  shares means nothing is locked).
- Positions that do hold shares should have `locked_collateral` recomputed
  via `calculate_locked_collateral(yes_shares, no_shares, market_price)` at
  whatever price the shares were actually bought at — not assumed to be
  50/50.
- There were no fixture/snapshot files checked into this repo at the time of
  this change (`tests/`, `contracts/market/src` contain only inline test
  data), so no files needed editing. Hand-built `Position { .. }` literals in
  existing tests already set `locked_collateral` to the share-based value
  directly and did not need changes. If you maintain seeded state outside
  this repo (e.g. a local devnet snapshot or fixture data), re-derive
  `locked_collateral` for every position using the rule above before
  diffing against post-fix behavior, or the diff will look like a
  regression when it is actually a correction.
