# Wire Collect Fee Callback on Withdraw - Feature Specification

**Status**: Proposed | Design Phase  
**Feature**: Wire collect_fee callback on withdraw  
**Date**: June 30, 2026  

---

## Overview

This feature wires the `collect_fee` callback from the Market contract to the Treasury contract during the `withdraw_unused_collateral()` flow. Currently, the Market contract computes fees but doesn't properly notify the Treasury contract about fee collection events.

## Motivation

Current State:
- Market contract computes withdrawal fees based on `fee_rate_bps`
- Fees are deducted from user withdrawal
- Treasury contract address is registered but not called
- Fee collection is not properly tracked on the Treasury

Desired State:
- Market contract invokes Treasury's `collect_fee()` function during withdrawal
- Treasury receives fee amount and metadata (market_id, user, amount)
- Fees are properly tracked and accumulated by Treasury
- Clear accounting of all collected fees

## Problem Statement

1. **Decoupling**: Market and Treasury contracts are not properly wired
2. **Tracking**: Treasury cannot track fees collected by Market
3. **Accountability**: No clear record of which market/user paid fees
4. **Events**: Fee collection not visible to off-chain services

## Proposed Design

### Current Flow (Before)

```
withdraw_unused_collateral(user, market_id, amount)
    │
    ├─ Calculate fee: fee = amount * fee_rate_bps / 10_000
    │
    ├─ If fee > 0 and treasury registered:
    │  └─ Transfer fee to treasury address (token transfer only)
    │
    ├─ Deduct from user's collateral
    │
    └─ Emit CollateralWithdrawnEvent
```

### New Flow (After)

```
withdraw_unused_collateral(user, market_id, amount)
    │
    ├─ Calculate fee: fee = amount * fee_rate_bps / 10_000
    │
    ├─ If fee > 0 and treasury registered:
    │  ├─ Transfer fee to treasury address (token transfer)
    │  │
    │  └─ ★ NEW: Invoke treasury.collect_fee()
    │     └─ Parameters:
    │        ├─ caller: current contract address
    │        ├─ collateral_token: market.collateral_token
    │        ├─ market_id: market_id
    │        └─ fee_amount: fee
    │
    ├─ Deduct from user's collateral
    │
    └─ Emit CollateralWithdrawnEvent + FeeCollectedEvent
```

### Implementation Details

#### 1. Invoke Treasury.collect_fee()

**Location**: `contracts/market/src/withdraw.rs` (after token transfer)

```rust
if fee_amount > 0 {
    if let Some(treasury_addr) = storage::get_treasury(&env) {
        // Transfer tokens
        token_client.transfer(&contract_address, &treasury_addr, &fee_amount);

        // ★ NEW: Invoke treasury callback
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
            contract_address.into_val(&env),
            market.collateral_token.clone().into_val(&env),
            market_id.into_val(&env),
            fee_amount.into_val(&env),
        ];
        let _: () = env.invoke_contract(
            &treasury_addr,
            &Symbol::new(&env, "collect_fee"),
            args,
        );
        
        // Emit fee collection event
        events::emit_fee_collected(&env, market_id, &user, fee_amount);
    }
}
```

#### 2. Emit Event on Fee Collection

**Location**: `contracts/market/src/events.rs`

```rust
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollectedEvent {
    #[topic]
    pub market_id: u32,
    pub user: Address,
    pub fee_amount: i128,
    pub collected_at: u64,
}

pub fn emit_fee_collected(
    env: &Env,
    market_id: u32,
    user: &Address,
    fee_amount: i128,
) {
    FeeCollectedEvent {
        market_id,
        user: user.clone(),
        fee_amount,
        collected_at: env.ledger().timestamp(),
    }
    .publish(env);
}
```

#### 3. Error Handling

Scenarios:
- ✓ Treasury not registered: Skip callback (no error)
- ✓ Treasury callback fails: Propagate error to user
- ✓ Zero fee: Skip callback
- ✓ Treasury not available: Propagate contract error

```rust
// Treasury callback error handling
match env.invoke_contract::<()>(
    &treasury_addr,
    &Symbol::new(&env, "collect_fee"),
    args,
) {
    Ok(_) => {
        // Success - fee collected
        events::emit_fee_collected(&env, market_id, &user, fee_amount);
    }
    Err(e) => {
        // Treasury callback failed
        return Err(e);
    }
}
```

#### 4. Contract Interaction

**Market Contract** (Caller)
- Location: `contracts/market/src/withdraw.rs`
- Action: Invoke `collect_fee()` on Treasury
- Parameters: caller, collateral_token, market_id, fee_amount

**Treasury Contract** (Receiver)
- Function: `collect_fee(caller: Address, collateral_token: Address, market_id: u32, fee_amount: i128)`
- Actions:
  - Verify caller is registered Market contract
  - Accumulate fees by market/collateral_token
  - Update treasury balance
  - Emit event for off-chain tracking

### Changes Required

#### Market Contract
1. **withdraw.rs**:
   - Add collect_fee() invocation after token transfer
   - Add error handling for callback failure
   - Add event emission

2. **events.rs**:
   - Add FeeCollectedEvent struct
   - Add emit_fee_collected() function

3. **lib.rs** (optional):
   - Add setter for treasury contract address if not exists
   - Keep existing get_treasury() function

#### Treasury Contract
1. **lib.rs**:
   - Ensure collect_fee() function is properly defined
   - Verify caller authorization
   - Accumulate fees properly
   - Emit FeeAccumulatedEvent

### Testing Strategy

#### Unit Tests (Market Contract)

1. **Test fee callback invoked**
   - Set up Treasury contract address
   - Withdraw with fee > 0
   - Verify collect_fee() called
   - Event emitted

2. **Test fee callback not invoked**
   - No Treasury registered
   - Withdraw proceeds normally
   - No callback error

3. **Test zero fee**
   - Fee rate = 0
   - No callback invoked
   - Withdrawal succeeds

4. **Test Treasury callback failure**
   - Treasury registered but unavailable
   - Callback fails
   - Error propagated to user

5. **Test fee amount calculation**
   - Verify correct fee passed to Treasury
   - Fee = amount * rate / 10_000

#### Integration Tests

1. **Full withdraw flow with Treasury**
   - User deposits collateral
   - Market registers Treasury
   - User withdraws with fee
   - Treasury receives callback
   - Fee properly accumulated

2. **Multiple withdrawals**
   - Multiple users withdraw
   - Treasury tracks all fees
   - Event history complete

3. **Fee rate changes**
   - Initial fee rate set
   - Withdrawals with rate 1
   - Rate updated
   - Subsequent withdrawals use new rate

### Event Catalog

New Event:
```
FeeCollectedEvent:
  - market_id (topic): Market where fee was collected
  - user: User who withdrew
  - fee_amount: Amount collected as fee
  - collected_at: Timestamp of collection
```

Updated Event (already exists):
```
CollateralWithdrawnEvent:
  - user (topic): User withdrawing
  - market_id (topic): Market withdrawing from
  - amount: Amount withdrawn (before fee deduction)
  - new_total: Remaining collateral after withdrawal and fee
```

### Configuration

No new configuration needed. Uses existing:
- `treasury_address`: Already registered via `set_treasury()`
- `fee_rate_bps`: Already set via `set_fee_rate()`

### Backward Compatibility

**Backward Compatible**: YES
- Existing withdrawals without Treasury continue to work
- New callback is optional (only invoked if Treasury registered)
- No changes to public API
- No breaking changes to state

### Migration/Deployment

**No migration needed**:
- Existing markets continue to work
- Treasury optional
- Callback only invoked if Treasury configured
- Can be added to existing deployments

### Success Criteria

✅ Market contract invokes collect_fee() callback  
✅ Treasury contract receives fee notifications  
✅ Events properly emitted for off-chain tracking  
✅ Error handling works correctly  
✅ Backward compatible with non-Treasury markets  
✅ Unit tests pass (5+ test cases)  
✅ Integration tests pass (3+ test cases)  
✅ Documentation complete  

### Files to Modify

1. **contracts/market/src/withdraw.rs** - Add callback invocation
2. **contracts/market/src/events.rs** - Add FeeCollectedEvent
3. **contracts/market/src/test.rs** - Add unit tests
4. **tests/withdraw_treasury_test.rs** - Add integration tests (existing file)

### Files to Create (Optional)

- None required (uses existing Treasury contract)

### Implementation Checklist

- [ ] Add collect_fee() invocation in withdraw.rs
- [ ] Add error handling for callback
- [ ] Add FeeCollectedEvent to events.rs
- [ ] Add emit_fee_collected() function
- [ ] Add unit tests (5 test cases)
- [ ] Add integration tests (3 test cases)
- [ ] Verify with Treasury contract stub
- [ ] Update documentation
- [ ] Build and test

### Related Issues/PRs

- Previous: `feat/market-metadata-uri` (storage v5)
- Depends on: Treasury contract with collect_fee() function
- Follow-up: Off-chain fee tracking and analytics

---

## Design Decisions

### Why Invoke Callback vs Just Transfer?

**Current approach (transfer only)**:
- Simple but decoupled
- Treasury unaware of individual fee collections
- No per-market/user tracking on Treasury

**Proposed approach (with callback)**:
- Treasury properly notified
- Can accumulate and track fees
- Enables future treasury analytics
- Maintains accounting integrity

We chose callback invocation because:
1. **Accountability**: Clear record of fee sources
2. **Tracking**: Treasury can track per-market fees
3. **Extensibility**: Treasury can perform additional logic
4. **Events**: Off-chain services have visibility

### Why Emit Event on Market Side?

**Benefits**:
- Market-side event visibility
- Can track all fees leaving contract
- Off-chain indexers track fee flow
- Enables market-level fee analytics

### Error Handling - Fail or Skip?

**Decision**: Fail if Treasury callback fails
**Rationale**:
- Better error visibility
- Prevents silent failures
- Users get clear feedback
- Matches authorization pattern

### Parameter Passing

**Via invoke_contract args**:
- caller: Market contract address
- collateral_token: Token used for fees
- market_id: Market where fees came from
- fee_amount: Amount collected

**Why these parameters**:
- Caller: Treasury can verify it's a registered Market
- Token: Treasury tracks fees per collateral type
- Market ID: Fee attribution to source
- Amount: Required for accounting

---

## Future Enhancements

1. **Fee Distribution**: Extend to distribute fees to protocol DAO
2. **Multi-tier Fees**: Different rates per market type
3. **Fee Analytics**: Dashboard showing fee flow by market
4. **Treasury Actions**: Keeper role to sweep accumulated fees
5. **Fee Governance**: DAO vote on fee rates

---

## Questions Addressed

**Q: What if Treasury is not registered?**
A: Callback is skipped, withdrawal proceeds normally. No error.

**Q: What if Treasury contract is unavailable?**
A: Callback fails, error is propagated to user. Withdrawal reverts.

**Q: Can this be deployed to existing contracts?**
A: Yes, backward compatible. Callback only invoked if Treasury configured.

**Q: How are fees tracked?**
A: Via FeeCollectedEvent and Treasury internal accounting.

**Q: What if callback fails?**
A: Error is returned to user. Withdrawal reverts.

---

**Document Status**: Design Review  
**Last Updated**: June 30, 2026  
**Branch**: `feat/wire-collect-fee-callback`
