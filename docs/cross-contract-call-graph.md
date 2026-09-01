# Cross-Contract Call Graph

Documents every cross-contract invocation in the Vatix protocol so
developers can reason about call chains, authorization requirements, and
failure modes without reading all four contract sources.

## Contracts

| Contract | Crate | Purpose |
|---|---|---|
| **Market** | `contracts/market` | Core trading, deposit/withdraw, oracle resolution, settlement |
| **Treasury** | `contracts/treasury` | Custody and accounting of protocol fees |
| **Outcome Token** | `contracts/outcome-token` | Mint/burn YES/NO share tokens per market |
| **Resolution** | `contracts/resolution` | Challenge-window lifecycle for oracle candidates |

---

## Call Graph

```
User
 │
 ├─ deposit_collateral(market_id, amount)
 │      Market (no cross-contract calls)
 │
 ├─ update_position(market_id, yes_delta, no_delta, price)
 │      Market
 │       ├─[if outcome_token_contract set]──► OutcomeToken::balance(market_id, user, Yes|No)
 │       │        [reconciliation guard — rejects with PositionTokenMismatch on divergence]
 │       └─[if outcome_token_contract set]──► OutcomeToken::mint(market_id, user, kind, amount)
 │                                            OutcomeToken::burn(market_id, user, kind, amount)
 │
 ├─ settle_position / batch_settle_positions / settle_positions_page(market_id)
 │      Market
 │       └─[if outcome_token_contract set]──► OutcomeToken::balance(market_id, user, Yes|No)
 │                [reconciliation guard — rejects/skips user on divergence]
 │
 ├─ withdraw_unused_collateral(market_id, amount)
 │      Market
 │       └─[if treasury set AND fee > 0]
 │              SAC token transfer: Market → Treasury (fee_amount)
 │              └──► Treasury::collect_fee(caller, token, market_id, fee_amount)
 │
 ├─ resolve_market(market_id, outcome, signature)
 │      Market (no cross-contract calls; oracle sig verified in-contract)
 │
 ├─ Admin: reconcile_position_tokens(market_id, user)
 │      Market
 │       └──► OutcomeToken::mint(market_id, user, kind, amount)
 │            OutcomeToken::burn(market_id, user, kind, amount)
 │            [mints/burns tokens to match Position; admin-gated repair path]
 │
 └─ Resolution lifecycle (off-chain orchestrator drives these)
        │
        ├─ Resolution::propose(proposer, market_id, outcome, signature, ...)
        │       └──► Market::verify_signature(market_id, outcome, signature)
        │            [rejects proposal early if oracle sig is invalid]
        │
        └─ Resolution::finalize(finalizer, candidate_id)
                └──► Market::resolve_market(market_id, outcome, signature)
                     [atomically settles the market after challenge window]
```

---

## Edge-by-Edge Reference

### 1. Market → OutcomeToken (mint/burn)

| Property | Value |
|---|---|
| Caller | `MarketContract::update_position` |
| Callee | `OutcomeTokenContract::mint` / `OutcomeTokenContract::burn` |
| Trigger | `yes_delta` or `no_delta` ≠ 0 AND `outcome_token_contract` is registered |
| Auth required | `OutcomeToken` checks `market_contract.require_auth()` — the Market contract must be the registered market |
| Failure behaviour | If the callee panics or returns an error the entire `update_position` call reverts |
| Registration | `MarketContract::set_outcome_token_contract(admin, address)` |

```
update_position(user, market_id, +yes_delta, 0, price)
  → OutcomeToken::mint(market_id, user, TokenKind::Yes, yes_delta)

update_position(user, market_id, -yes_delta, 0, price)
  → OutcomeToken::burn(market_id, user, TokenKind::Yes, yes_delta)
```

### 2. Market → Treasury (fee routing)

| Property | Value |
|---|---|
| Caller | `MarketContract::withdraw_unused_collateral` |
| Callee | `TreasuryContract::collect_fee` |
| Trigger | Fee rate > 0 AND treasury address is registered AND computed fee > 0 |
| Token transfer | SAC `transfer(market_contract → treasury, fee_amount)` happens *before* `collect_fee` is called |
| Auth required | `Treasury` checks `caller == authorized_market_contract` |
| Failure behaviour | If `collect_fee` reverts the token transfer also reverts (same transaction) |
| Registration | `MarketContract::set_treasury(admin, address)` + `MarketContract::set_fee_rate(admin, bps)` |

```
withdraw_unused_collateral(user, market_id, amount)
  → SAC::transfer(market → treasury, fee)
  → Treasury::collect_fee(market, token, market_id, fee)
```

### 3. Resolution → Market (signature pre-validation)

| Property | Value |
|---|---|
| Caller | `ResolutionContract::propose` |
| Callee | `MarketContract::verify_signature` |
| Trigger | Always — every `propose` call pre-validates the oracle signature |
| Purpose | Reject invalid signatures at proposal time, not at finalize time |
| Auth required | None (read-only verification) |
| Failure behaviour | If verification fails `propose` returns `ContractError::InvalidSignature` |
| Registration | `ResolutionContract::initialize(admin, factory, market_contract)` |

```
Resolution::propose(proposer, market_id, outcome, signature, ...)
  → Market::verify_signature(market_id, outcome, signature)
```

### 4. Resolution → Market (finalize → resolve)

| Property | Value |
|---|---|
| Caller | `ResolutionContract::finalize` |
| Callee | `MarketContract::resolve_market` |
| Trigger | Challenge window has closed AND candidate is not challenged AND signature is not expired |
| Purpose | Atomically mark the market as resolved on-chain after the dispute window |
| Auth required | `Market::resolve_market` requires `resolver.require_auth()` — the Resolution contract's address acts as resolver |
| Failure behaviour | If `resolve_market` reverts (e.g. already resolved) the entire `finalize` call reverts |
| Registration | `ResolutionContract::initialize(admin, factory, market_contract)` |

```
Resolution::finalize(finalizer, candidate_id)
  → (candidate.status = Finalized)
  → Market::resolve_market(market_id, outcome, signature)
```

### 5. Market ↔ OutcomeToken (dual-ledger reconciliation)

| Property | Value |
|---|---|
| Caller | `MarketContract::update_position`, `settle_position` (+ batch/page variants), `get_position_token_parity`, `reconcile_position_tokens` |
| Callee | `OutcomeTokenContract::balance` (read), plus `mint`/`burn` from `reconcile_position_tokens` |
| Trigger | Every trade and settlement attempt (guard check); admin-initiated repair |
| Purpose | `Position` (Market storage) and `OutcomeToken` balances are two ledgers that are supposed to always agree — a historical bug, partial upgrade, or manual admin mint/burn issued directly on the outcome-token contract can make them diverge. The guard reads both ledgers and refuses to proceed on mismatch instead of silently re-syncing |
| Auth required | Guard reads (`balance`) need no auth. `reconcile_position_tokens` requires the Market contract's stored admin |
| Failure behaviour | `update_position`/`settle_position` reject with `ContractError::PositionTokenMismatch` (after emitting `PositionTokenMismatchDetected`) when the two ledgers disagree for that user/market. `batch_settle_positions`/`settle_positions_page` skip the affected user rather than aborting the whole batch |
| Repair | `MarketContract::reconcile_position_tokens(admin, market_id, user)` mints/burns `OutcomeToken` balances to match `Position` (Position is the source of truth — see `contracts/market/src/reconciliation.rs`), emitting `PositionTokensReconciled` |
| Registration | Reuses the existing `set_outcome_token_contract` wiring — no separate registration step |

```
update_position(user, market_id, yes_delta, no_delta, price)
  → OutcomeToken::balance(market_id, user, Yes|No)   [parity check, both sides]
  → reject with PositionTokenMismatch if divergent, else proceed as in edge 1

settle_position(user, market_id) / batch_settle_positions / settle_positions_page
  → OutcomeToken::balance(market_id, user, Yes|No)   [parity check]
  → reject (single) or skip (batch/page) with PositionTokenMismatch if divergent

reconcile_position_tokens(admin, market_id, user)     [admin-gated]
  → OutcomeToken::mint / OutcomeToken::burn            [bring balances back to Position]
```

---

Registering a resolution contract selects this challenge-based lifecycle as the
market's exclusive resolution mode. While the registration is present,
`Market::resolve_market_threshold` fails closed with
`ResolutionNotFinalized`; a threshold quorum therefore cannot bypass an open or
challenged candidate. Threshold resolution remains available only when no
resolution contract is registered. After `finalize` resolves the market through
the guarded single-signature callback, any later resolution attempt fails with
`MarketAlreadyResolved`.

## Authorization Summary

| Call | Who authorizes |
|---|---|
| `OutcomeToken::mint` / `burn` | Market contract address (`market_contract.require_auth()`) |
| `Treasury::collect_fee` | Market contract address (`caller == authorized_market_contract`) |
| `Market::verify_signature` | No auth (public read) |
| `Market::resolve_market` (via Resolution) | Resolution contract address as `resolver` |
| `Market::reconcile_position_tokens` | Market contract's stored admin |
| `Market::get_position_token_parity` | No auth (public read) |
| `Market::resolve_market_threshold` | Caller authorizes as `resolver`; disabled while a Resolution contract is registered |

---

## Registration Prerequisites

All cross-contract wiring is opt-in and admin-controlled. No calls are made
unless the relevant address has been registered:

```
# Wire outcome tokens
MarketContract::set_outcome_token_contract(admin, outcome_token_address)
OutcomeTokenContract::set_market_contract(admin, market_address)

# Wire treasury fee routing
MarketContract::set_treasury(admin, treasury_address)
MarketContract::set_fee_rate(admin, fee_rate_bps)          # 0 = disabled
TreasuryContract::initialize(admin, market_address)         # or set_market_contract

# Wire resolution gating
ResolutionContract::initialize(admin, factory, market_address)
MarketContract::set_resolution_contract(admin, resolution_address)
```
