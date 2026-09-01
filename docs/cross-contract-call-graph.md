# Cross-Contract Call Graph

> **Audit status:** production ABI — reflects `vatix-market-contract` as implemented in
> `contracts/market/src/`.  This document supersedes any earlier mock-ABI versions.

## Scope

The Vatix v1 on-chain surface is a **single deployed contract** —
`vatix-market-contract` — which is the entry point for all market lifecycle
operations.  There are **no inter-contract calls between custom Vatix contracts**
in the current production code.

The only external contract call made by `vatix-market-contract` is to the
**Stellar Asset Contract (SAC)** for the market's collateral token (e.g. USDC),
via the standard `soroban_sdk::token` interface.

---

## Contract map

```
┌──────────────────────────────────────────────────────────────────┐
│  vatix-market-contract  (contracts/market/src/lib.rs)            │
│                                                                  │
│  Public entrypoints (production ABI)                             │
│  ──────────────────────────────────────────────────────────────  │
│  initialize_market         admin-only, writes Market storage     │
│  deposit_collateral        user-auth,  calls SAC::transfer       │
│  withdraw_unused_collateral user-auth,  calls SAC::transfer      │
│  resolve_market            no Soroban auth (oracle sig only)     │
│  queue_fee_rate_change     admin-only, writes PendingFeeRate     │
│  apply_pending_fee_rate    admin-only, writes FeeRateBps         │
│  get_fee_rate_bps          read-only,  no auth                   │
│  get_position              read-only,  no auth                   │
│  add_fee_waiver            admin-only, writes FeeWaivers         │
│  remove_fee_waiver         admin-only, writes FeeWaivers         │
│                                                                  │
│  External call targets                                           │
│  ──────────────────────────────                                  │
│  SAC (Stellar Asset Contract)                                    │
│    token::Client::transfer(from, to, amount)                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## Entrypoint call paths

### `initialize_market`

```
initialize_market(creator, question, end_time, oracle_pubkey, collateral_token)
  └─ creator.require_auth()
  └─ storage::get_admin()                     [read]
  └─ validation::validate_market_creation()
  └─ storage::increment_market_id()           [write: MarketCounter]
  └─ storage::set_market()                    [write: Market(id)]
  └─ events::emit_market_created()
```

### `deposit_collateral`

```
deposit_collateral(user, market_id, amount)
  └─ user.require_auth()
  └─ validation::validate_collateral_amount()
  └─ storage::get_market()                    [read]
  └─ SAC::transfer(user → contract)           ← only cross-contract call
  └─ storage::get_position()                  [read]
  └─ storage::set_position()                  [write: Position(market_id, user)]
  └─ events::emit_collateral_deposited()
```

### `withdraw_unused_collateral`

```
withdraw_unused_collateral(user, market_id, amount)
  └─ user.require_auth()
  └─ validation::validate_collateral_amount()
  └─ storage::get_market()                    [read]
  └─ storage::get_position()                  [read]
  └─ positions::calculate_locked_collateral()
  └─ storage::set_position()                  [write: Position(market_id, user)]
  └─ SAC::transfer(contract → user)           ← only cross-contract call
  └─ events::emit_collateral_withdrawn()
```

**CEI note:** state is updated before the SAC transfer — this satisfies the
Checks-Effects-Interactions pattern.  See `docs/reentrancy-cei-audit.md`.

### `resolve_market`

```
resolve_market(market_id_str, outcome, signature)
  └─ validation::parse_market_id()
  └─ storage::get_market()                    [read]
  └─ oracle::verify_oracle_signature()
  │   └─ env.crypto().ed25519_verify()        [host fn — not a cross-contract call]
  └─ storage::set_market()                    [write: Market(id)]
  └─ events::emit_market_resolved()
```

**Auth note:** no `require_auth` — authorization is implicit in the Ed25519
oracle signature stored in the market at creation.

### `queue_fee_rate_change`

```
queue_fee_rate_change(caller, new_rate_bps)
  └─ caller.require_auth()
  └─ storage::get_admin()                     [read]
  └─ [validate: new_rate_bps ≤ FEE_RATE_MAX_BPS=500]
  └─ env.ledger().timestamp()                 [clock: ledger time]
  └─ storage::set_pending_fee_rate()          [write: PendingFeeRate]
  └─ events::emit_fee_rate_change_queued()
```

Timelock: `FEE_RATE_TIMELOCK_SECONDS = 172_800` (48 h).

### `apply_pending_fee_rate`

```
apply_pending_fee_rate(caller)
  └─ caller.require_auth()
  └─ storage::get_admin()                     [read]
  └─ storage::get_pending_fee_rate()          [read]
  └─ env.ledger().timestamp()                 [clock: LEDGER TIME — not wall clock]
  └─ storage::set_fee_rate_bps()              [write: FeeRateBps]
  └─ storage::clear_pending_fee_rate()        [delete: PendingFeeRate]
  └─ events::emit_fee_rate_applied()
```

### `get_fee_rate_bps`

```
get_fee_rate_bps()
  └─ storage::get_fee_rate_bps()              [read-only, no auth]
```

### `get_position`

```
get_position(market_id, user)
  └─ storage::get_position()                  [read-only, no auth]
```

Canonical source of truth for YES/NO share balances.  Off-chain indexers
(`vatix-backend.UserPosition`) **must reconcile** against this entrypoint.
Regression: `regression_get_position_is_canonical_source_of_truth` in `test.rs`.

### `add_fee_waiver`

```
add_fee_waiver(caller, waiver_address)
  └─ caller.require_auth()
  └─ storage::get_admin()                     [read]
  └─ storage::get_fee_waivers()               [read]
  └─ [cap check: len < MAX_FEE_WAIVERS=100]
  └─ storage::set_fee_waivers()               [write: FeeWaivers]
  └─ events::emit_fee_waiver_added()
```

### `remove_fee_waiver`

```
remove_fee_waiver(caller, waiver_address)
  └─ caller.require_auth()
  └─ storage::get_admin()                     [read]
  └─ storage::get_fee_waivers()               [read]
  └─ storage::set_fee_waivers()               [write: FeeWaivers]
  └─ events::emit_fee_waiver_removed()
```

---

## Share accounting source of truth

| Layer | Location | Authoritative? |
|---|---|---|
| On-chain storage | `StorageKey::Position(market_id, user)` | **Yes — canonical** |
| Public entrypoint | `get_position(market_id, user)` | Yes (reads storage) |
| Off-chain indexer | `vatix-backend.UserPosition` | No — must reconcile |

Any `UserPosition` record that disagrees with `get_position` is stale or
incorrect.  The `regression_get_position_is_canonical_source_of_truth` test
in `contracts/market/src/test.rs` is the CI sentinel for this invariant.

---

## Known design gaps

| # | Gap | Status |
|---|---|---|
| 1 | `resolve_market` has no `require_auth` — relies solely on Ed25519 sig | Intentional oracle model; document risk in audit report |
| 2 | `ed25519_verify` panics on invalid signature instead of returning `Err` | SDK limitation; see `oracle.rs` TODO |
| 3 | Per-market isolated collateral (no cross-market pool) | Open — post-MVP |
| 4 | No outcome token minting in current ABI | Planned — `outcome-token` crate |

---

## Out of scope (v1)

- `contracts/treasury` — not yet deployed
- `contracts/resolution` — not yet deployed
- `contracts/outcome-token` — not yet deployed
- Off-chain indexer (`vatix-backend`) — separate repository
