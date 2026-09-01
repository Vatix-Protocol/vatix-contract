# AUTH_TABLE — Entrypoints, Auth Requirements, and Storage Mutations

Contract: `vatix-market-contract`  
Source: `contracts/market/src/`  
Updated: 2026-09-01

> Legend:
> - **Auth** — how the call is authorized (`require_auth`, oracle signature, or none)
> - **Reads** — `StorageKey` variants read during execution
> - **Writes** — `StorageKey` variants written (or deleted) during execution
> - **External calls** — cross-contract calls made

---

## Entrypoint table

| Entrypoint | Auth | Reads | Writes | External calls |
|---|---|---|---|---|
| `initialize_market` | `creator.require_auth()` + must equal stored `Admin` | `Admin`, `MarketCounter` | `MarketCounter`, `Market(id)` | — |
| `deposit_collateral` | `user.require_auth()` | `Market(id)`, `Position(id, user)` | `Position(id, user)` | `SAC::transfer(user → contract)` |
| `withdraw_unused_collateral` | `user.require_auth()` | `Market(id)`, `Position(id, user)` | `Position(id, user)` | `SAC::transfer(contract → user)` |
| `resolve_market` | Ed25519 oracle signature (no Soroban auth) | `Market(id)` | `Market(id)` | — |
| `queue_fee_rate_change` | `caller.require_auth()` + must equal stored `Admin` | `Admin` | `PendingFeeRate` | — |
| `apply_pending_fee_rate` | `caller.require_auth()` + must equal stored `Admin` | `Admin`, `PendingFeeRate` | `FeeRateBps`, deletes `PendingFeeRate` | — |
| `get_fee_rate_bps` | None (read-only) | `FeeRateBps` | — | — |
| `get_position` | None (read-only) | `Position(id, user)` | — | — |
| `add_fee_waiver` | `caller.require_auth()` + must equal stored `Admin` | `Admin`, `FeeWaivers` | `FeeWaivers` | — |
| `remove_fee_waiver` | `caller.require_auth()` + must equal stored `Admin` | `Admin`, `FeeWaivers` | `FeeWaivers` | — |

---

## Admin gating

All **admin-only** entrypoints follow the same pattern:

```rust
caller.require_auth();
let admin = storage::get_admin(env);
if *caller != admin {
    return Err(ContractError::NotAdmin);
}
```

There is currently **no entrypoint to rotate the admin**; `Admin` is set once
at contract initialization via `storage::set_admin`. Any admin rotation requires
a contract upgrade.  This is tracked as a known gap.

---

## Timelock invariant

`queue_fee_rate_change` and `apply_pending_fee_rate` are the only admin mutators
that affect the fee rate.  The change is gated by a **172 800-second (48 h)
timelock** checked against `env.ledger().timestamp()` — not wall-clock time.

```
No new instant admin mutators without a 172 800 s timelock where peers already
use 172 800 s.
```

Current compliance: **met** — `add_fee_waiver` and `remove_fee_waiver` are
immediate (no timelock), but they are list-management operations, not fee
mutations.  Fee-rate changes are the only category requiring a timelock, and
both queue/apply enforce `FEE_RATE_TIMELOCK_SECONDS = 172_800`.

---

## Storage key inventory

| Key | Type | Durability | Description |
|---|---|---|---|
| `Admin` | `Address` | Persistent | Contract admin address |
| `MarketCounter` | `u32` | Persistent | Auto-increment market ID counter |
| `Market(u32)` | `Market` | Persistent | Full market state per market ID |
| `Position(u32, Address)` | `Position` | Persistent | User position per (market, user) |
| `FeeRateBps` | `u32` | Persistent | Current fee rate in basis points (0–500) |
| `PendingFeeRate` | `PendingFeeRate` | Persistent | Queued fee-rate change (cleared on apply) |
| `FeeWaivers` | `Vec<Address>` | Persistent | Bounded list of fee-waiver addresses (max 100) |

---

## Error code quick reference

| Code | Name | Raised by |
|---|---|---|
| 1 | `MarketNotFound` | Any entrypoint given a non-existent market ID |
| 2 | `MarketAlreadyResolved` | `resolve_market` when market is already resolved |
| 3 | `MarketNotResolved` | Settlement when market not yet resolved |
| 4 | `MarketExpired` | (reserved) |
| 5 | `MarketNotActive` | `deposit_collateral`, `withdraw_unused_collateral` on non-active market |
| 10 | `InsufficientCollateral` | `withdraw_unused_collateral` when amount > available |
| 11 | `PositionAlreadySettled` | Settlement on already-settled position |
| 12 | `NoPositionFound` | (reserved) |
| 13 | `InvalidShareAmount` | Share validation failures |
| 20 | `InvalidSignature` | Zero oracle pubkey on market creation |
| 21 | `UnauthorizedOracle` | `verify_oracle_signature` with zero pubkey |
| 22 | `InvalidOutcome` | (reserved) |
| 30 | `InvalidPrice` | (reserved) |
| 31 | `InvalidQuantity` | Amount ≤ 0, negative, or out of range |
| 32 | `InvalidTimestamp` | `end_time` in past or > 1 year future |
| 33 | `InvalidQuestion` | Empty or ≥ 500 char question |
| 40 | `Unauthorized` | (reserved) |
| 41 | `NotAdmin` | Admin-only entrypoints called by non-admin |
| 50 | `TokenTransferFailed` | (reserved — SAC panics rather than errors in practice) |
| 60 | `ArithmeticOverflow` | `checked_add` overflow on collateral arithmetic |
| 70 | `FeeRateTimelockNotExpired` | `apply_pending_fee_rate` before timelock expires, or no pending change |
| 71 | `FeeRateOutOfRange` | `queue_fee_rate_change` with rate > 500 bps |
| 80 | `FeeWaiverCapReached` | `add_fee_waiver` when list is at MAX_FEE_WAIVERS=100 |
