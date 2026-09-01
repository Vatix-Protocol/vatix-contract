# Events Reference

Contract: `vatix-market-contract`  
Source: `contracts/market/src/events.rs`  
Updated: 2026-09-01

All events are emitted via the Soroban `#[contractevent]` macro which serialises
structs into `(topics, data)` pairs.  Fields annotated `#[topic]` appear in the
topics vector; remaining fields appear in the data map.

Topic 0 is always the **event name symbol** (e.g. `"market_created_event"`),
followed by any `#[topic]`-annotated fields in struct-declaration order.

---

## Market events

### `MarketCreatedEvent`

Emitted by: `initialize_market`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"market_created_event"` |
| topic[1] | `u32` | `market_id` |
| data.question | `String` | Market question text |
| data.end_time | `u64` | Unix timestamp (ledger time) when trading closes |

---

### `MarketResolvedEvent`

Emitted by: `resolve_market`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"market_resolved_event"` |
| topic[1] | `u32` | `market_id` |
| data.outcome | `bool` | `true` = YES won, `false` = NO won |
| data.resolved_at | `u64` | `env.ledger().timestamp()` at resolution |

---

## Collateral events

### `CollateralDepositedEvent`

Emitted by: `deposit_collateral`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"collateral_deposited_event"` |
| topic[1] | `Address` | `user` |
| topic[2] | `u32` | `market_id` |
| data.amount | `i128` | Stroops deposited in this call |
| data.new_total | `i128` | User's `total_deposited` after this call |

---

### `CollateralWithdrawnEvent`

Emitted by: `withdraw_unused_collateral`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"collateral_withdrawn_event"` |
| topic[1] | `Address` | `user` |
| topic[2] | `u32` | `market_id` |
| data.amount | `i128` | Stroops withdrawn in this call |
| data.new_total | `i128` | User's `total_deposited` after this call |

---

### `WithdrawEdgeCaseEvent`

Emitted by: `withdraw_unused_collateral` when `total_deposited == 0`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"withdraw_edge_case_event"` |
| topic[1] | `Address` | `user` |
| topic[2] | `u32` | `market_id` |
| data.amount | `i128` | Amount that was attempted |

This event fires *before* the `InsufficientCollateral` error is returned; it
records the attempted withdrawal amount for off-chain observability.

---

## Position events

### `PositionUpdatedEvent`

Emitted by: `positions::update_position` (internal, called by share-buying logic)

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"position_updated_event"` |
| topic[1] | `u32` | `market_id` |
| topic[2] | `Address` | `user` |
| data.yes_shares | `i128` | New total YES share balance |
| data.no_shares | `i128` | New total NO share balance |
| data.locked_collateral | `i128` | Collateral now locked to cover net position |

---

### `PositionLimitExceededEvent`

Emitted by: `positions::update_position` when a share delta would make a balance
go below zero (before the error is returned).

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"position_limit_exceeded_event"` |
| topic[1] | `u32` | `market_id` |
| topic[2] | `Address` | `user` |
| data.side_yes | `bool` | `true` if the YES side would go negative, `false` for NO |

---

### `PositionSettledEvent`

Emitted by: `settlement::execute_settlement`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"position_settled_event"` |
| topic[1] | `u32` | `market_id` |
| topic[2] | `Address` | `user` |
| data.payout | `i128` | Stroops transferred to the user |
| data.settled_at | `u64` | `env.ledger().timestamp()` at settlement |

---

## Oracle events

### `OracleSignatureVerifiedEvent`

Emitted by: `oracle::verify_oracle_signature` (internal)

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"oracle_signature_verified_event"` |
| topic[1] | `u32` | `market_id` |
| data.outcome | `bool` | Verified outcome |
| data.verified_at | `u64` | `env.ledger().timestamp()` at verification |

---

## Validation events

### `ValidationFailedEvent`

Emitted by: `events::emit_validation_failed` at various validation sites.

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"validation_failed_event"` |
| topic[1] | `Symbol` | `context` — identifies the failing validation site |
| data.error_code | `u32` | Numeric `ContractError` discriminant |

---

## Fee rate events

### `FeeRateChangeQueuedEvent`

Emitted by: `queue_fee_rate_change`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"fee_rate_change_queued_event"` |
| topic[1] | `Address` | `queued_by` (admin) |
| data.new_rate_bps | `u32` | Proposed fee rate in basis points |
| data.effective_at | `u64` | `queued_at + 172_800` (ledger timestamp) |

---

### `FeeRateAppliedEvent`

Emitted by: `apply_pending_fee_rate`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"fee_rate_applied_event"` |
| topic[1] | `Address` | `applied_by` (admin) |
| data.new_rate_bps | `u32` | New fee rate now in effect |
| data.applied_at | `u64` | `env.ledger().timestamp()` at application |

---

## Fee waiver events

### `FeeWaiverAddedEvent`

Emitted by: `add_fee_waiver`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"fee_waiver_added_event"` |
| topic[1] | `Address` | `added_by` (admin) |
| topic[2] | `Address` | `waiver_address` |
| data.waiver_count | `u32` | Number of waiver addresses after this addition |

---

### `FeeWaiverRemovedEvent`

Emitted by: `remove_fee_waiver`

| Position | Type | Value |
|---|---|---|
| topic[0] | `Symbol` | `"fee_waiver_removed_event"` |
| topic[1] | `Address` | `removed_by` (admin) |
| topic[2] | `Address` | `waiver_address` |
| data.waiver_count | `u32` | Number of waiver addresses after this removal |

---

## Indexer notes

- All timestamps are `env.ledger().timestamp()` (ledger close time), never wall-clock time.
- `new_total` in deposit/withdraw events equals `position.total_deposited` after the operation; it does NOT include locked collateral from share positions — use `get_position` for the full position state.
- Events are emitted after all state writes; an event being present guarantees the
  corresponding storage mutation succeeded.
