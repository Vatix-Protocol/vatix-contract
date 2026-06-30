# Close Market to Deposits Feature Implementation

## Overview
This document describes the implementation of the "close market to new deposits" feature for the Vatix Market contract. This feature allows administrators to prevent new collateral deposits into a market while preserving all other functionality (trading, withdrawals, and settlement).

## Feature Scope

### What This Feature Does
- **Admin Control**: Administrators can invoke `close_market_to_deposits()` to prevent new deposits
- **Preserve Functionality**: Existing positions can still be traded, and collateral can still be withdrawn
- **Event Tracking**: Emits `MarketClosedToDepositsEvent` when a market is closed
- **Idempotent**: Closing an already-closed market is a no-op (succeeds silently)

### What This Feature Does NOT Do
- Does NOT prevent market resolution or settlement
- Does NOT cancel existing positions
- Does NOT force withdrawals
- Does NOT change the market status (remains `Active`)

## Implementation Details

### 1. Type System Changes (`types.rs`)
Added a new field to the `Market` struct:
```rust
/// Flag indicating whether the market is closed to new deposits.
/// When true, users cannot deposit new collateral, but can still withdraw and trade.
pub closed_to_deposits: bool,
```

### 2. Error Handling (`error.rs`)
Added a new error code for deposit rejection:
```rust
/// Market is closed to new deposits.
///
/// The market has been administratively closed to prevent new collateral deposits,
/// though existing positions can still be traded and withdrawn.
MarketClosedToDeposits = 6,
```

### 3. Deposit Validation (`deposit.rs`)
Added a check in the `deposit_collateral()` function:
```rust
if market.closed_to_deposits {
    return Err(ContractError::MarketClosedToDeposits);
}
```

This check runs **after** general market validation but **before** the token transfer.

### 4. Event Emission (`events.rs`)
Added a new event structure:
```rust
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketClosedToDepositsEvent {
    #[topic]
    pub market_id: u32,
    pub admin: Address,
    pub closed_at: u64,
}
```

With emitter function:
```rust
pub fn emit_market_closed_to_deposits(
    env: &Env,
    market_id: u32,
    admin: &Address,
    closed_at: u64,
)
```

### 5. Admin Function (`lib.rs`)
Implemented the main admin function:
```rust
pub fn close_market_to_deposits(
    env: Env,
    admin: Address,
    market_id: u32,
) -> Result<(), ContractError>
```

**Behavior**:
1. Requires admin authorization (calls `admin.require_auth()`)
2. Verifies caller is the stored admin
3. Loads the market from storage
4. Sets `closed_to_deposits = true`
5. Persists the updated market
6. Emits `MarketClosedToDepositsEvent`

**Authorization**: Only the contract admin can call this function.

### 6. Market Initialization (`lib.rs`)
When new markets are created in `initialize_market()`, the new field is initialized:
```rust
closed_to_deposits: false,
```

### 7. Storage Version Bump (`storage.rs`)
Updated `STORAGE_VERSION` from 3 to 4 to reflect the struct layout change. This ensures:
- Old deployments cannot read markets with the new field
- New deployments will reject any call to a contract with the old version
- Testnet deployments require reinitialization to migrate to the new version

## Testing

### Unit Tests (in `test.rs`)
Comprehensive tests covering:
- ✓ `test_close_market_to_deposits_success` - Verifies market transitions to closed state
- ✓ `test_close_market_to_deposits_not_admin_fails` - Authorization check
- ✓ `test_close_market_to_deposits_not_found_fails` - Market existence validation
- ✓ `test_deposit_rejected_when_closed_to_deposits` - Deposit rejection with error code 6
- ✓ `test_withdraw_still_works_after_close_to_deposits` - Withdrawal compatibility
- ✓ `test_multiple_close_market_to_deposits_idempotent` - Idempotent behavior
- ✓ `test_close_market_event_emitted` - Event emission verification

### Integration Tests (in `tests/close_market_test.rs`)
High-level tests covering:
- ✓ `close_market_to_deposits_succeeds` - Full workflow with event emission
- ✓ `deposit_fails_when_market_closed_to_deposits` - Deposit rejection in integrated environment
- ✓ `withdrawal_succeeds_when_market_closed_to_deposits` - Withdrawal compatibility
- ✓ `close_market_to_deposits_idempotent` - Idempotent behavior at integration level
- ✓ `unauthorized_close_market_to_deposits_fails` - Authorization in integrated environment
- ✓ `close_nonexistent_market_to_deposits_fails` - Market existence in integrated environment

## Files Modified

### Core Implementation
1. **contracts/market/src/types.rs** - Added `closed_to_deposits: bool` field to Market struct
2. **contracts/market/src/error.rs** - Added `MarketClosedToDeposits = 6` error code
3. **contracts/market/src/deposit.rs** - Added validation check before deposit
4. **contracts/market/src/events.rs** - Added event structure and emitter
5. **contracts/market/src/lib.rs** - Added `close_market_to_deposits()` admin function
6. **contracts/market/src/storage.rs** - Bumped STORAGE_VERSION from 3 to 4

### Testing
7. **contracts/market/src/test.rs** - Added 7 comprehensive unit tests
8. **tests/close_market_test.rs** - New file with 6 integration tests

## API Reference

### Function Signature
```rust
pub fn close_market_to_deposits(
    env: Env,
    admin: Address,
    market_id: u32,
) -> Result<(), ContractError>
```

### Parameters
- **env**: Soroban contract environment
- **admin**: The caller's address (must be the stored admin)
- **market_id**: The ID of the market to close to deposits

### Return Values
- **Ok(())**: Successfully closed market to deposits
- **Err(ContractError::NotAdmin)**: Caller is not the admin
- **Err(ContractError::MarketNotFound)**: Market with given ID does not exist

### Event Emitted
```rust
MarketClosedToDepositsEvent {
    market_id: u32,      // Market that was closed
    admin: Address,      // Admin address that closed it
    closed_at: u64,      // Ledger timestamp when closed
}
```

## Backward Compatibility

### Storage Migration
The feature requires a storage version bump. To deploy on testnet:

1. **Increment** `STORAGE_VERSION` in `storage.rs` (already done: 3 → 4)
2. **Rebuild** contract: `stellar contract build`
3. **Redeploy** to testnet
4. **Reinitialize** contract (call `initialize(admin)`)

Old deployments will reject all storage calls with `UpgradeRequired` error until reinitialized.

### New Market Behavior
- All newly created markets have `closed_to_deposits = false` (default open)
- Existing markets (if somehow preserved across versions) would not have this field and would fail storage reads

## Security Considerations

1. **Authorization**: Only the admin can close markets to deposits
   - Uses `require_auth()` for strong authorization
   - Validates caller against stored admin address

2. **State Immutability**: Once closed, only admin can modify market state
   - No way for users to reopen markets
   - Deposits are permanently blocked until admin action

3. **Atomic Operations**: 
   - Market state update and event emission are tightly coupled
   - No partial failures possible

4. **No Collateral Risk**:
   - Existing collateral remains accessible (withdrawals still work)
   - No funds are frozen or lost

## Future Enhancements

Potential improvements not included in this implementation:
- Add `reopen_market_to_deposits()` function to allow reopening
- Add timestamp tracking for when market was closed
- Add reason/memo field for why market was closed
- Emit additional events when deposits are rejected (for analytics)
- Add migration helpers for off-chain services to track closed markets

## Verification Checklist

✓ Market struct includes `closed_to_deposits` field  
✓ New error code added and tested  
✓ Deposit validation checks `closed_to_deposits` flag  
✓ Event structure and emitter implemented  
✓ Admin function implemented with proper authorization  
✓ New markets initialize with `closed_to_deposits = false`  
✓ Storage version bumped  
✓ 7 unit tests added and passing  
✓ 6 integration tests added and passing  
✓ No breaking changes to existing functionality  
✓ Withdrawals still work when market is closed  
✓ Idempotent behavior verified  

## Migration Guide for Off-Chain Services

### Event Indexing
Subscribe to `MarketClosedToDepositsEvent` to track when markets are closed:
```typescript
// Example: Listen for market closure events
const events = await client.events({
  contract: marketContractId,
  type: 'contract-emitted',
  name: 'MarketClosedToDepositsEvent',
});

events.forEach(event => {
  const { market_id, admin, closed_at } = event.data;
  console.log(`Market ${market_id} closed at ${closed_at}`);
});
```

### UI Changes
- Display a "closed to deposits" badge on markets with `closed_to_deposits = true`
- Disable deposit buttons for closed markets
- Show informational message: "This market is closed to new deposits. Existing positions can still be traded and withdrawn."

### Error Handling
Handle `ContractError::MarketClosedToDeposits = 6`:
```typescript
if (error.code === 6) {
  displayError('This market is currently closed to new deposits');
}
```

---

**Implementation Date**: June 30, 2026  
**Feature Status**: ✅ Complete  
**Storage Version**: 4  
**Compatibility**: Requires reinitialization on existing deployments
