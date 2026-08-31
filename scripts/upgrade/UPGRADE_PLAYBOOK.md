# Vatix Protocol Upgrade Playbook

This document defines the strict order and preconditions for cross-contract upgrades in the Vatix protocol.

## Critical Safety Requirement: Upgrade Order

**ABSOLUTE RULE**: Upgrades to enable oracle adapters must follow this exact order. Deviation **will brick share minting**.

### Upgrade Sequence

1. **Outcome Token Contract** (initializes first)
   - Status: Ready for oracle adapter integration
   - Action: Set oracle adapter registry reference
   - Side effects: None on market contract

2. **Treasury Contract** (initializes second)
   - Status: Ready for oracle adapter integration
   - Action: Register fee routes to adapters
   - Side effects: None on market contract

3. **Resolution Contract** (initializes third)
   - Status: To be implemented
   - Action: Register supported oracle adapters (Reflector, Pyth, etc.)
   - Side effects: Signals to market contract that adapters are live

4. **Market Contract** (final)
   - Status: Ready to lock oracle mode
   - Action: Resolution contract calls `enable_oracle_adapters()` on market contract
   - Side effects: Ed25519 fallback permanently disabled for this contract instance

### What Each Step Does

#### Step 3 → Step 4 Handoff: Oracle Adapter Enablement

```rust
// Resolution contract triggers this after adapter registration
market_contract.enable_oracle_adapters()

// Inside market contract storage:
storage::enable_oracle_adapters(env)  // Sets persistent flag

// From this point forward:
if storage::has_oracle_adapters(env) {
    // Ed25519 verification ALWAYS fails
    return Err(ContractError::UnauthorizedOracle)
}
```

This creates a **fail-closed** security model:
- If oracle adapters are enabled, Ed25519 is **rejected immediately**
- Prevents silent fallback during incomplete deployments
- Makes incomplete upgrades detectable (resolution fails, not silently succeeds)

## Why This Order Matters

### Scenario: Wrong Order (Steps 2→4 only, skipping 3)

```
Step 1 ✅ Outcome Token: adapters registered
Step 2 ✅ Treasury: fee routes configured
Step 4 ❌ Market: enable_oracle_adapters() called TOO EARLY

Problem: No resolution contract has registered adapters yet!
Result: Markets cannot be resolved via adapters (fallback to Ed25519)
Impact: Trading continues with old oracle model, defeating upgrade
```

### Scenario: Correct Order

```
Step 1 ✅ Outcome Token: adapters registered
Step 2 ✅ Treasury: fee routes configured
Step 3 ✅ Resolution: adapters registered (Reflector, Pyth)
Step 4 ✅ Market: enable_oracle_adapters() called NOW SAFE

All systems aligned: New oracle model active, Ed25519 disabled
```

## Storage Versioning

The market contract's persistent storage uses versioned keys to support future upgrades:

```rust
#[contracttype]
pub enum StorageKey {
    Market(u32),              // v1: Market state
    Position(u32, Address),   // v1: User positions
    Admin,                    // v1: Admin address
    MarketCounter,            // v1: Market ID counter
    OracleAdapters,           // v2: Oracle adapter flag (NEW)
}
```

**Migration strategy for future versions**:
1. Add new `StorageKey` variant (e.g., `OracleAdaptersV2`)
2. Add migration function to copy data
3. Update storage getters to prefer new key, fall back to old

This ensures contracts can be upgraded without losing state.

## Deployment Checklist

Before enabling oracle adapters in production:

- [ ] All four contracts compiled and verified
- [ ] Resolution contract has ≥1 registered adapter (Reflector or Pyth)
- [ ] Market contract has `enable_oracle_adapters()` marked as callable only by authorized account
- [ ] Testnet deployment successful with adapter-to-market resolution flow
- [ ] Mainnet rehearsal: deploy in order, verify resolution works
- [ ] Audit sign-off on upgrade procedure and fail-closed guarantees

## Rollback Procedure

If oracle adapters fail to activate correctly:

1. **Do NOT upgrade market contract alone** — this will disable Ed25519
2. Resolution contract upgrade can be rolled back independently
3. Market contract can be redeployed with `OracleAdapters` key removed from storage
4. Outcome token and treasury are not affected

---

## References

- Market contract oracle module: [contracts/market/src/oracle.rs](../../contracts/market/src/oracle.rs)
- Storage versioning: [contracts/market/src/storage.rs](../../contracts/market/src/storage.rs)
- Fail-closed security model: [docs/SECURITY.md](../../docs/SECURITY.md)
- Issue tracking: [#139 - Decentralized Oracle Integration](https://github.com/Vatix-Protocol/vatix-contract/issues/139)
