# Security Model & Fail-Closed Oracle Adapters

This document explains the security guarantees and design decisions in the Vatix market contract, with emphasis on the fail-closed oracle adapter transition.

## Executive Summary

The market contract implements a **fail-closed oracle security model**:
- **Before adapters**: Ed25519 signatures are accepted for market resolution
- **After adapters**: Ed25519 is **permanently rejected**, forcing resolution through new oracle contracts
- **This prevents**: Silent fallback to old oracle method during incomplete cross-contract upgrades

**Test evidence**: See `test_verify_fails_closed_when_adapters_enabled()` and `test_upgrade_order_safety_adapters_must_be_enabled_first()` in [contracts/market/src/oracle.rs](../contracts/market/src/oracle.rs).

---

## Fail-Closed Behavior

### The Problem: Incomplete Upgrades

Imagine upgrading from Ed25519 oracle to decentralized oracle adapters (Reflector/Pyth). If done wrong:

```
Step 1 ✅ Deploy resolution contract with adapters
Step 2 ❌ OOPS: Forgot to upgrade market contract

Result:
  - Market tries to resolve using resolution contract adapters
  - Adapters don't find market reference (old version)
  - Falls back to Ed25519 signature verification
  - Market resolves with old method (BUG!)
  - No one notices until later (DISASTER)
```

### The Solution: Fail-Closed Lock

Once oracle adapters are registered, Ed25519 **cannot** be used:

```rust
// In market contract
if storage::has_oracle_adapters(env) {
    return Err(ContractError::UnauthorizedOracle);  // ALWAYS FAILS
}
// Never reaches Ed25519 verification below
```

With this in place:

```
Step 1 ✅ Deploy resolution contract with adapters
Step 2 ❌ OOPS: Forgot to upgrade market contract
Step 3 ❌ Market resolution fails (no adapters registered yet)
Step 4 👀 Team notices failure in staging
Step 5 ✅ Deploy market contract upgrade
Step 6 ✅ Market resolution works
```

**Key insight**: Incomplete upgrade is **detectable** (fails) instead of **silent** (succeeds wrong).

---

## Implementation Details

### Storage: Persistent Adapter Flag

```rust
#[contracttype]
pub enum StorageKey {
    OracleAdapters,  // Persistent boolean flag
}

pub fn enable_oracle_adapters(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::OracleAdapters, &true);
    // Once set, CANNOT be unset (by design)
}

pub fn has_oracle_adapters(env: &Env) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::OracleAdapters)
}
```

**Why persistent?** Cannot accidentally revert due to cache/transaction rollback.

### Verification: Zero Compromise

```rust
pub fn verify_oracle_signature(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signature: &BytesN<64>,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    // FAIL-CLOSED: reject if adapters enabled
    if crate::storage::has_oracle_adapters(env) {
        return Err(ContractError::UnauthorizedOracle);  // ← FIRST CHECK
    }

    // Only if no adapters: check zero key
    if oracle_pubkey == &BytesN::from_array(env, &[0u8; 32]) {
        return Err(ContractError::UnauthorizedOracle);
    }

    // Only if no adapters: verify Ed25519
    let message = construct_oracle_message(env, market_id, outcome);
    env.crypto()
        .ed25519_verify(oracle_pubkey, &message.into(), signature);

    Ok(())
}
```

**Order matters**: Check adapters first, before any crypto operations.

---

## Cross-Contract Upgrade Guarantee

The upgrade sequence ensures safety:

```
Market Contract     Treasury         Resolution        Outcome Token
───────────────     ────────────     ──────────        ──────────────
 (V1: Ed25519)      (V1: Routes)     (V1: Ready)       (V1: Ready)
        ↓                ↓                 ↓                  ↓
   [Upgrade]        [Upgrade]        [Upgrade]         [Upgrade]
        ↓                ↓                 ↓                  ↓
(V2: Adapters off) (V2: Register)  (V2: Adapters)    (V2: Registry)
                         ↓                 ↓                  
                    enable_oracle_    (called by          
                    adapters() ◄───── resolution)         
                         ↓                                   
                    LOCK Ed25519                           
```

See [UPGRADE_PLAYBOOK.md](../scripts/upgrade/UPGRADE_PLAYBOOK.md) for detailed upgrade steps.

---

## Reentrancy Protection

The market contract is **safe from reentrancy** due to transfer-first pattern:

```rust
pub fn deposit_collateral(
    env: Env,
    user: Address,
    market_id: u32,
    amount: i128,
) -> Result<(), ContractError> {
    user.require_auth();
    
    let market = storage::get_market(&env, market_id)
        .ok_or(ContractError::MarketNotFound)?;
    
    // ... validation ...
    
    // TRANSFER FIRST
    collateral_token.transfer(
        &user,
        &env.current_contract_address(),
        &amount,
    )?;
    
    // UPDATE STATE (after transfer succeeds)
    let mut position = storage::get_position(&env, market_id, &user)
        .unwrap_or_default();
    position.available_collateral += amount;
    storage::set_position(&env, market_id, &user, &position);
    
    // EMIT EVENT (final)
    events::emit_deposit_event(&env, &user, market_id, amount);
    
    Ok(())
}
```

**Why safe?**
1. Collateral token transfer is **atomic** (Soroban guarantee)
2. State update happens **after** transfer
3. Even if token contract reenters, market position hasn't changed yet
4. Attacker cannot exploit race condition

---

## Input Validation Security

### Market Creation Validation

```rust
pub fn validate_market_creation(
    question: &String,
    end_time: u64,
    current_time: u64,
) -> Result<(), ContractError> {
    // Question length: 1-499 characters
    let len = question.len();
    if len == 0 || len >= 500 {
        return Err(ContractError::InvalidQuestion);
    }

    // End time: must be in future
    if end_time <= current_time {
        return Err(ContractError::InvalidTimestamp);
    }

    // End time: must be ≤ 1 year in future (prevents permanent lock)
    if end_time - current_time > 365 * 24 * 60 * 60 {
        return Err(ContractError::InvalidTimestamp);
    }

    Ok(())
}
```

### Oracle Key Validation

```rust
// All-zero key can never produce valid Ed25519 signature
if oracle_pubkey == BytesN::from_array(&env, &[0u8; 32]) {
    return Err(ContractError::InvalidSignature);
}
```

---

## Error Codes

Critical error codes for security:

| Code | Meaning | Implication |
|------|---------|-------------|
| `UnauthorizedOracle` (59) | Oracle verification failed (including fail-closed) | Market cannot be resolved; triggers alert |
| `NotAdmin` (64) | Non-admin tried to create market | Access control working |
| `InvalidTimestamp` (54) | Market time bounds violated | Input validation working |
| `MarketNotFound` (52) | Referenced market doesn't exist | Prevents orphaned positions |
| `Unauthorized` (1) | User auth failed | Caller identity mismatch |

---

## Audit Checklist

- [x] Fail-closed oracle adapter behavior implemented
- [x] Upgrade order documented in UPGRADE_PLAYBOOK.md
- [x] Ed25519 zero-key validation in place
- [x] Persistent adapter flag (cannot revert)
- [x] Reentrancy protection via transfer-first pattern
- [x] Input validation for market creation
- [x] Authorization checks on all state-changing operations
- [x] Test coverage for fail-closed behavior (3 tests)
- [ ] Mainnet rehearsal of upgrade procedure
- [ ] External audit sign-off

---

## References

- Fail-closed oracle tests: [contracts/market/src/oracle.rs#L245-L269](../contracts/market/src/oracle.rs#L245-L269)
- Authorization table: [docs/AUTH_TABLE.md](./AUTH_TABLE.md)
- Upgrade procedure: [scripts/upgrade/UPGRADE_PLAYBOOK.md](../scripts/upgrade/UPGRADE_PLAYBOOK.md)
- Issue tracking: [#139 - Decentralized Oracle Integration](https://github.com/Vatix-Protocol/vatix-contract/issues/139)
