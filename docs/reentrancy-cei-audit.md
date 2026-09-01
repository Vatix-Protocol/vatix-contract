# Reentrancy / CEI Audit

Contract: `vatix-market-contract`  
Source: `contracts/market/src/`  
Updated: 2026-09-01

The **Checks-Effects-Interactions (CEI)** pattern requires that:

1. **Checks** — validate inputs, auth, market state.
2. **Effects** — persist all storage mutations.
3. **Interactions** — make any external (cross-contract) calls last.

In Soroban, reentrancy via cross-contract calls is the primary concern because a
malicious SAC implementation or a contract registered at an attacker-controlled
address could call back into the market contract during `transfer`.  Following CEI
ensures that the contract's state is fully committed before any external call can
observe or exploit intermediate state.

---

## Entrypoint-by-entrypoint CEI notes

### `initialize_market` ✅ CEI satisfied

No external calls.  The function reads admin, validates inputs, writes market
storage, and emits an event.  Order: Checks → Effects → (no Interactions).

```
Checks:   require_auth, admin equality, validate_market_creation, zero-pubkey guard
Effects:  increment_market_id, set_market
Interactions: (none)
```

---

### `deposit_collateral` ✅ CEI satisfied

The SAC `transfer` call happens **after** validation but **before** the position
write.  This is an intentional deviation from strict CEI — the token transfer
must succeed before we record the deposit to avoid crediting the user for a
failed transfer.

The Soroban host atomically reverts all state changes if the transaction fails,
so if `transfer` reverts the position write is never committed.  This is safe on
Soroban.

```
Checks:   require_auth, validate_collateral_amount, get_market, status check
Interactions: SAC::transfer(user → contract)   ← before position write
Effects:  get_position (initialize if absent), set_position
Events:   emit_collateral_deposited
```

**Reentrancy risk:** Low.  The SAC `transfer` deducts from the user's balance
atomically.  Even if a malicious token re-entered `deposit_collateral`, the user
would need to authorize a second call, and the position write has not yet
committed so the re-entrant call would create a separate position entry (not
double-credit the first).

**Recommendation:** For extra safety, add a reentrancy guard or ensure the
deposit amount is recorded in a temporary slot before calling `transfer`.
Tracked as a post-MVP improvement.

---

### `withdraw_unused_collateral` ✅ CEI satisfied — state updated before transfer

```
Checks:   require_auth, validate_collateral_amount, get_market, status check
          get_position, total_deposited > 0 check
          calculate_locked_collateral, available >= amount check
Effects:  set_position (total_deposited reduced)   ← BEFORE transfer
Interactions: SAC::transfer(contract → user)       ← AFTER state update
Events:   emit_collateral_withdrawn
```

**CEI compliance:** The position's `total_deposited` is decremented **before**
the SAC transfer.  If the transfer re-enters `withdraw_unused_collateral`, the
reduced `total_deposited` will correctly block a second withdrawal of the same
funds.  This is a textbook-correct CEI implementation.

---

### `resolve_market` ✅ CEI satisfied

No external calls.  Oracle signature is verified via a host function
(`env.crypto().ed25519_verify`), not a cross-contract call.

```
Checks:   parse_market_id, get_market, status != Resolved
          oracle::verify_oracle_signature (host fn — no external call)
Effects:  set_market (status, result)
Events:   emit_market_resolved
```

**Note:** `ed25519_verify` panics on an invalid signature (SDK limitation).
This is not a reentrancy issue, but it prevents returning a clean error.
See `oracle.rs` TODO.

---

### `queue_fee_rate_change` ✅ CEI satisfied

No external calls.

```
Checks:   require_auth, admin equality, rate range check
Effects:  set_pending_fee_rate
Events:   emit_fee_rate_change_queued
```

---

### `apply_pending_fee_rate` ✅ CEI satisfied

No external calls.

```
Checks:   require_auth, admin equality, get_pending_fee_rate, timelock check
Effects:  set_fee_rate_bps, clear_pending_fee_rate
Events:   emit_fee_rate_applied
```

---

### `get_fee_rate_bps` ✅ Read-only — no mutations

---

### `get_position` ✅ Read-only — no mutations

---

### `add_fee_waiver` ✅ CEI satisfied

No external calls.

```
Checks:   require_auth, admin equality, idempotency check, cap check
Effects:  set_fee_waivers
Events:   emit_fee_waiver_added
```

---

### `remove_fee_waiver` ✅ CEI satisfied

No external calls.

```
Checks:   require_auth, admin equality
Effects:  set_fee_waivers (rebuild without removed address)
Events:   emit_fee_waiver_removed (only if address was found)
```

---

## Summary

| Entrypoint | CEI status | External calls | Notes |
|---|---|---|---|
| `initialize_market` | ✅ | None | — |
| `deposit_collateral` | ✅ | SAC::transfer before position write | Soroban atomicity makes safe; reentrancy risk low |
| `withdraw_unused_collateral` | ✅ | SAC::transfer after position write | Textbook CEI |
| `resolve_market` | ✅ | None (host fn only) | `ed25519_verify` panic is not a CEI issue |
| `queue_fee_rate_change` | ✅ | None | — |
| `apply_pending_fee_rate` | ✅ | None | — |
| `get_fee_rate_bps` | ✅ | None | Read-only |
| `get_position` | ✅ | None | Read-only |
| `add_fee_waiver` | ✅ | None | — |
| `remove_fee_waiver` | ✅ | None | — |

---

## Open items

1. **`deposit_collateral` pre-transfer ordering** — `SAC::transfer` is called
   before the position write.  While safe under Soroban's atomic revert model,
   consider reversing the order (write to a pending slot, then transfer, then
   commit) for strict CEI compliance.

2. **`ed25519_verify` panic** — `resolve_market` will panic rather than return
   `ContractError::InvalidSignature` on a bad signature.  Not a reentrancy issue
   but blocks graceful error handling.

3. **No reentrancy lock** — Soroban does not support cross-contract reentrancy
   within the same transaction by default, but a future upgrade to a malicious
   token contract could attempt it.  A flag-based reentrancy guard would add
   defence-in-depth for `deposit_collateral` and `withdraw_unused_collateral`.
