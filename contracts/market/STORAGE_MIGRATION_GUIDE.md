# Storage Migration Guide

> **Comprehensive guide for handling storage version bumps in the Vatix Market Contract**

## Table of Contents

1. [Overview](#overview)
2. [When to Bump Storage Version](#when-to-bump-storage-version)
3. [Migration Procedures](#migration-procedures)
4. [Testing Migrations](#testing-migrations)
5. [Rollback and Recovery](#rollback-and-recovery)
6. [Common Pitfalls](#common-pitfalls)
7. [Version History](#version-history)

---

## Overview

### What is Storage Versioning?

The Vatix Market Contract uses a storage versioning mechanism to ensure data integrity across contract upgrades. The `STORAGE_VERSION` constant in `src/storage.rs` acts as a compatibility lock:

```rust
pub const STORAGE_VERSION: u32 = 3;
```

Every storage operation calls `assert_version()` to verify the on-chain version matches the code version. Mismatches return `ContractError::UpgradeRequired`, preventing operations on incompatible data.

### Why Storage Versioning Matters

**Without versioning:**
- New contract code could misinterpret old storage layouts
- Field additions/removals could cause silent data corruption
- Type changes could lead to deserialization errors
- No mechanism to detect incompatible deployments

**With versioning:**
- Explicit compatibility checking
- Safe contract upgrades
- Clear migration paths
- Protection against accidental downgrades

---

## When to Bump Storage Version

### Always Bump When

✅ **Adding new fields to stored types**
```rust
// OLD
pub struct Position {
    market_id: u32,
    user: Address,
    yes_shares: i128,
    no_shares: i128,
}

// NEW - Requires version bump
pub struct Position {
    market_id: u32,
    user: Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128, // NEW FIELD
}
```

✅ **Removing fields from stored types**
```rust
// Removing any field requires version bump
```

✅ **Changing field types**
```rust
// OLD
pub struct Market {
    end_time: u64,
}

// NEW - Requires version bump
pub struct Market {
    end_time: i64, // Type changed
}
```

✅ **Renaming fields (semantic change)**
```rust
// OLD
pub struct Position {
    collateral: i128,
}

// NEW - Requires version bump
pub struct Position {
    total_deposited: i128, // Renamed/semantic change
}
```

✅ **Adding new storage keys**
```rust
// OLD
pub enum StorageKey {
    Market(u32),
    Position(u32, Address),
}

// NEW - Requires version bump
pub enum StorageKey {
    Market(u32),
    Position(u32, Address),
    Treasury, // NEW KEY
}
```

✅ **Changing how existing fields are calculated or used**
```rust
// Example: locked_collateral now derived from shares, not deposits
// Even if the type doesn't change, the semantics do
```

### Safe to Skip

❌ **Adding new functions that don't touch storage**
- Pure computation functions
- View functions that only read existing data

❌ **Fixing bugs that don't change data layout**
- Logic fixes that maintain existing storage semantics
- Event emission changes
- Error message updates

❌ **Documentation changes**
- Comment updates
- Inline documentation

---

## Migration Procedures

### For Testnet Deployments

#### 1. Prepare the Migration

**Step 1.1: Increment the version**
```rust
// src/storage.rs
pub const STORAGE_VERSION: u32 = 4; // Incremented from 3
```

**Step 1.2: Document the change**

Create or update `MIGRATION.md` with:
- What changed in the storage layout
- Why the change was necessary
- Impact on existing data
- Migration steps for data

Example entry:
```markdown
## Version 3 → 4: Added Treasury Integration

### What Changed
- Added `Treasury` storage key
- Added `FeeRateBps` storage key
- Market contract can now route fees to treasury

### Data Migration
No existing data affected. New fields are optional and default to None/0.

### Migration Steps
1. Increment `STORAGE_VERSION` to 4
2. Redeploy contract
3. Call `initialize(admin)` on new deployment
4. Configure treasury with `set_treasury(address)`
```

**Step 1.3: Update tests**

Ensure tests in `src/storage.rs` verify version checking:
```rust
#[test]
fn test_wrong_version_returns_upgrade_required() {
    let env = Env::default();
    let contract_id = env.register(MarketContract, ());
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&StorageKey::StorageVersion, &0u32);
        assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
    });
}
```

#### 2. Build and Deploy

**Step 2.1: Build the new WASM**
```bash
cd contracts/market
stellar contract build
# or
make build
```

**Step 2.2: Deploy to testnet**
```bash
# Set your testnet credentials
export TESTNET_SECRET_KEY="S..."

# Deploy the new WASM
stellar contract deploy \
    --wasm target/wasm32v1-none/release/vatix_market_contract.wasm \
    --source $TESTNET_SECRET_KEY \
    --network testnet

# Output: New contract ID
# CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
```

**Step 2.3: Initialize the new deployment**
```bash
# Call initialize with admin address
stellar contract invoke \
    --id CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC \
    --source $TESTNET_SECRET_KEY \
    --network testnet \
    -- initialize \
    --admin GADMIN...

# This writes the new STORAGE_VERSION to the new deployment
```

#### 3. Verify the Migration

**Step 3.1: Check version is set**
```bash
# Query storage version (if you have a getter function)
stellar contract invoke \
    --id CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC \
    --network testnet \
    -- get_admin

# Should succeed, confirming version is correct
```

**Step 3.2: Test basic operations**
```bash
# Try creating a market
stellar contract invoke \
    --id CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC \
    --source $TESTNET_SECRET_KEY \
    --network testnet \
    -- initialize_market \
    --creator GADMIN... \
    --question "Test market?" \
    --end_time 1735689600 \
    --oracle_pubkey AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA \
    --collateral_token CUSDC...
```

**Step 3.3: Verify old deployment is locked**
```bash
# Try calling the old deployment
stellar contract invoke \
    --id OLD_CONTRACT_ID \
    --network testnet \
    -- get_market \
    --market_id 1

# Should fail with UpgradeRequired error
```

#### 4. Update Frontend and Services

**Step 4.1: Update contract addresses**
- Update environment variables with new contract ID
- Update configuration files
- Regenerate TypeScript bindings if needed

**Step 4.2: Test integration**
- Verify frontend can call new contract
- Test all critical user flows
- Ensure old contract calls fail gracefully

### For Mainnet Deployments

⚠️ **CRITICAL: Mainnet requires data migration strategy**

#### Pre-Deployment Checklist

- [ ] Migration thoroughly tested on testnet
- [ ] Data migration script prepared (if needed)
- [ ] Rollback plan documented
- [ ] User communication plan ready
- [ ] Monitoring and alerting configured
- [ ] Multi-signature admin approval obtained

#### Mainnet Migration Options

**Option 1: Fresh Deployment (Recommended for breaking changes)**

Best when existing data cannot be preserved or migrated.

```bash
# 1. Announce maintenance window to users
# 2. Deploy new contract
# 3. Initialize with admin
# 4. Update all services/frontend
# 5. Announce new contract address
```

**Option 2: Data Migration Script (Complex, not recommended)**

Requires writing a custom migration contract that:
1. Reads data from old contract
2. Transforms data to new format
3. Writes data to new contract

This is complex and error-prone. Only use if data preservation is critical.

---

## Testing Migrations

### Unit Tests for Version Checking

Always maintain these tests in `src/storage.rs`:

```rust
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_wrong_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_missing_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn migration_after_set_version_storage_is_accessible() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        
        env.as_contract(&contract_id, || {
            set_version(&env);
            assert_eq!(assert_version(&env), Ok(()));
            
            // Storage operations should now work
        });
    }

    #[test]
    fn migration_future_version_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &StorageKey::StorageVersion, 
                &(STORAGE_VERSION + 1)
            );
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }
}
```

### Integration Test Strategy

Create integration tests that simulate version mismatches:

```rust
#[test]
#[should_panic(expected = "Error(Contract, #70)")]
fn test_old_contract_rejects_operations_after_version_bump() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    
    // Simulate old deployment by setting old version
    env.as_contract(&contract_id, || {
        storage::set_admin(&env, &Address::generate(&env));
        env.storage().persistent().set(
            &storage::StorageKey::StorageVersion, 
            &(storage::STORAGE_VERSION - 1)
        );
    });
    
    // This should fail with UpgradeRequired (#70)
    client.get_market(&1u32);
}
```

### Manual Testing Checklist

Before deploying to testnet:

- [ ] Version constant incremented
- [ ] All storage types updated
- [ ] Migration documented in MIGRATION.md
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Contract builds successfully
- [ ] Clippy passes with no warnings
- [ ] Format check passes

After deploying to testnet:

- [ ] New deployment initialized successfully
- [ ] Storage operations work on new deployment
- [ ] Old deployment returns UpgradeRequired errors
- [ ] Frontend integrates with new contract
- [ ] All critical user flows tested

---

## Rollback and Recovery

### If Migration Fails on Testnet

**Immediate Actions:**
1. **Do NOT deploy to mainnet**
2. Investigate the failure root cause
3. Fix the issue in code
4. Re-test on local environment
5. Deploy again to testnet with fixes

**Common Failure Scenarios:**

**Scenario 1: Version not set properly**
```rust
// Problem: Forgot to call set_version in initialize
pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
    admin.require_auth();
    storage::set_admin(&env, &admin);
    // Missing: storage::set_version(&env);
    Ok(())
}

// Solution: Add set_version call
storage::set_version(&env);
```

**Scenario 2: Storage accessor skips version check**
```rust
// Problem: New function doesn't call assert_version
pub fn get_new_field(env: &Env) -> Option<u32> {
    // Missing: assert_version(env)?;
    env.storage().persistent().get(&StorageKey::NewField)
}

// Solution: Add version assertion
pub fn get_new_field(env: &Env) -> Result<Option<u32>, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().get(&StorageKey::NewField))
}
```

### If Migration Fails on Mainnet

⚠️ **CRITICAL SITUATION** - Follow incident response plan:

1. **Immediate communication**
   - Notify all users of the issue
   - Provide status updates every 30 minutes
   - Set expectations for resolution timeline

2. **Assess impact**
   - Can users withdraw funds?
   - Are positions locked?
   - Is data corrupted or just inaccessible?

3. **Emergency response options**

   **Option A: Quick fix and redeploy**
   - Fix the bug immediately
   - Deploy new contract with incremented version
   - Migrate data if possible

   **Option B: Restore from backup**
   - Deploy previous working version
   - Restore from known good state
   - Resume operations with old version

   **Option C: Data recovery script**
   - Write custom contract to extract data
   - Transform and migrate to new deployment
   - Verify all data transferred correctly

4. **Post-incident review**
   - Document what went wrong
   - Update testing procedures
   - Add safeguards to prevent recurrence

---

## Common Pitfalls

### Pitfall 1: Forgetting to Increment Version

**Problem:**
```rust
// Changed storage layout but forgot to bump version
pub struct Position {
    // ... existing fields ...
    new_field: i128, // ADDED but version not bumped!
}
```

**Consequence:** Deserialization errors, silent data corruption

**Prevention:**
- Always bump version when changing storage types
- Add a pre-commit hook to check for type changes
- Code review checklist includes version check

### Pitfall 2: Skipping Version Check in New Functions

**Problem:**
```rust
pub fn new_storage_function(env: &Env) {
    // Missing assert_version(env)?;
    env.storage().persistent().set(&StorageKey::NewKey, &value);
}
```

**Consequence:** New functions work on old deployments, creating inconsistent state

**Prevention:**
- Template for new storage functions includes version check
- Clippy rule to detect missing version checks (if possible)
- Code review specifically checks for version assertions

### Pitfall 3: Version Drift Across Contracts

**Problem:**
- Market contract at version 4
- Treasury contract at version 2
- Resolution contract at version 1
- No coordination between versions

**Consequence:** Incompatible contracts, integration failures

**Prevention:**
- Maintain a version compatibility matrix
- Document inter-contract dependencies
- Test contract integrations after version bumps

### Pitfall 4: Not Testing Old Deployment Lockout

**Problem:**
- Deployed new version
- Old version still accessible
- Users confused about which to use

**Consequence:** Split state, user confusion, support burden

**Prevention:**
- Always test that old deployment returns UpgradeRequired
- Update documentation/frontend immediately
- Monitor old contract for unexpected calls

### Pitfall 5: Insufficient Migration Documentation

**Problem:**
- Version bumped
- No documentation of what changed
- No migration steps provided

**Consequence:** Future developers don't understand migration history

**Prevention:**
- Always update MIGRATION.md with each version bump
- Include "what, why, how" for each change
- Link to relevant PRs/issues

---

## Version History

### Version 3 (Current)

**Date:** 2024-Q4
**Changes:**
- Added Treasury integration (StorageKey::Treasury, StorageKey::FeeRateBps)
- Added Outcome Token integration (StorageKey::OutcomeTokenContract)
- Added Resolution Contract integration (StorageKey::ResolutionContract)
- Added multi-signer threshold support (StorageKey::ThresholdSigners, StorageKey::ThresholdQuorum)

**Migration:** Fresh deployment required. No data migration available.

**Breaking Changes:** None (additive only)

---

### Version 2

**Date:** 2024-Q3
**Changes:**
- Fixed `locked_collateral` semantics (#262)
- `locked_collateral` now derived from shares only
- Removed direct collateral locking on deposit

**Migration:** Existing positions must recalculate `locked_collateral` using `calculate_locked_collateral(yes_shares, no_shares, market_price)`

**Breaking Changes:** 
- Positions with `locked_collateral == total_deposited` and no shares are incorrect
- Must be recomputed to `locked_collateral = 0`

**Reference:** See `MIGRATION.md` for detailed migration steps

---

### Version 1

**Date:** 2024-Q2
**Changes:**
- Initial storage layout
- Basic Market and Position types
- Admin and market counter storage

**Migration:** N/A (initial version)

---

## Best Practices Summary

### Development Phase

✅ **DO:**
- Increment version for any storage layout change
- Document changes in MIGRATION.md
- Add tests for version checking
- Test migration on local environment first
- Update all storage accessors to check version
- Add comprehensive inline documentation

❌ **DON'T:**
- Skip version bump for "small" changes
- Deploy without testing version lockout
- Forget to initialize new deployment
- Remove old MIGRATION.md entries
- Assume old data is compatible

### Deployment Phase

✅ **DO:**
- Test on testnet first
- Verify old deployment is locked
- Update frontend/services immediately
- Monitor for errors after deployment
- Communicate changes to users
- Keep deployment scripts version controlled

❌ **DON'T:**
- Deploy to mainnet without testnet validation
- Leave old contract accessible
- Deploy during high-traffic periods (mainnet)
- Skip backup/rollback planning
- Assume migration will go smoothly

### Post-Deployment Phase

✅ **DO:**
- Monitor error rates and logs
- Verify all integrations working
- Update documentation
- Conduct post-deployment review
- Archive old contract details
- Update SDK/client libraries

❌ **DON'T:**
- Forget to update version compatibility docs
- Leave old contract IDs in documentation
- Skip user communication about changes
- Ignore unexpected error patterns

---

## Additional Resources

### Code References

- **Storage Module:** `contracts/market/src/storage.rs`
- **Error Types:** `contracts/market/src/error.rs`
- **Migration History:** `contracts/market/MIGRATION.md`
- **Storage Tests:** `contracts/market/src/storage.rs#test`

### External Documentation

- [Soroban Storage Documentation](https://developers.stellar.org/docs/smart-contracts/data/storing-data)
- [Soroban Contract Lifecycle](https://developers.stellar.org/docs/smart-contracts/smart-contract-lifecycle)
- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/cli)

### Support

- **GitHub Issues:** [vatix-contract/issues](https://github.com/Vatix-Protocol/vatix-contract/issues)
- **Discussions:** [vatix-contract/discussions](https://github.com/Vatix-Protocol/vatix-contract/discussions)

---

## Changelog

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2026-06-29 | 1.0.0 | Team | Initial comprehensive migration guide |

---

**Last Updated:** 2026-06-29
**Document Version:** 1.0.0
**Contract Version:** 3
