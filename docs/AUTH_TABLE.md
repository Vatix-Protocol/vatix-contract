# Authorization Table (AUTH_TABLE)

This document defines the authorization model for all entry points in the Vatix market contract.

## Authorization Matrix

| Entry Point | Function | Caller Requirement | Auth Method | Notes |
|---|---|---|---|---|
| `initialize_market` | Create new market | **Admin only** | `caller.require_auth()` + admin check | Returns market ID; emits `MarketCreatedEvent` |
| `deposit_collateral` | Deposit collateral | **Market participant** | `caller.require_auth()` | User authorizes their own deposit; market must be active |
| `withdraw_unused_collateral` | Withdraw unused funds | **Position owner** | `caller.require_auth()` | User withdraws their own available balance; position must exist |
| `resolve_market` | Submit oracle signature | **Any caller** (signature authenticated) | Ed25519 signature on message | Only valid oracle pubkey + valid signature accepted; caller identity irrelevant |
| `query_market` | Read market state | **Public** | None (read-only) | Returns market details, no write authority needed |
| `query_position` | Read user position | **Position owner** | Address check (permission-less read) | Position data readable by any caller for any address |

## Auth Layers

### Layer 1: Admin Control
Only the admin address can create markets. The admin is set once during contract initialization and cannot be changed without a new deployment.

```rust
pub fn initialize_market(env: Env, creator: Address, ...) {
    creator.require_auth();        // Caller must sign
    let admin = storage::get_admin(&env);
    if creator != admin {
        return Err(ContractError::NotAdmin);
    }
}
```

**Who is admin?**
- Defined at contract deployment time
- Typically a controlled account (multisig or timelock)
- Cannot be changed (contract upgrade required)

### Layer 2: User Authority (Deposit/Withdraw)
Users authorize their own deposit/withdrawal operations by signing.

```rust
pub fn deposit_collateral(env: Env, user: Address, market_id: u32, amount: i128) {
    user.require_auth();  // User must sign this transaction
    // ... transfer tokens, record position
}
```

**Security model**:
- No one can deposit on behalf of another user
- No one can withdraw on behalf of another user
- User bears responsibility for amount and market selection

### Layer 3: Oracle Signature (Resolution)
Market resolution requires a valid Ed25519 signature from the registered oracle pubkey.

```rust
pub fn resolve_market(env: Env, market_id: u32, outcome: bool, signature: BytesN<64>) {
    // ... cryptographic verification
    env.crypto().ed25519_verify(&oracle_pubkey, &message, &signature);
}
```

**Why Ed25519?**
- Off-chain oracle signs the outcome with a private key
- Market contract verifies the signature matches the registered oracle pubkey
- Caller identity is irrelevant (anyone can submit a valid signature)

**Fail-Closed Mode**:
When oracle adapters are enabled, Ed25519 verification is **rejected**. This ensures:
- Markets cannot be resolved with old oracle method
- Forces resolution through new oracle adapter contracts
- Prevents silent fallback during incomplete upgrades

See [UPGRADE_PLAYBOOK.md](../upgrade/UPGRADE_PLAYBOOK.md).

### Layer 4: No Auth for Queries
Market and position queries are permission-less. Any caller can read any market or position.

```rust
pub fn query_position(env: Env, market_id: u32, user: Address) -> Option<Position> {
    // No authorization check — data is public
    storage::get_position(&env, market_id, &user)
}
```

**Rationale**:
- Markets are public information
- User positions are public (like on-chain trades in AMMs)
- Read-only operations carry no risk

## Events

All state-changing operations emit events for indexing:

| Event | Entry Point | Payload |
|---|---|---|
| `MarketCreatedEvent` | `initialize_market` | `market_id`, `question`, `end_time` |
| `DepositEvent` | `deposit_collateral` | `user`, `market_id`, `amount`, `timestamp` |
| `WithdrawEvent` | `withdraw_unused_collateral` | `user`, `market_id`, `amount`, `timestamp` |
| `ResolutionEvent` | `resolve_market` | `market_id`, `outcome`, `timestamp` |
| `OracleAdaptersEnabled` | (internal) | Emitted when adapters are locked in |

Event indexers use these to reconstruct user activity and market state.

## Cross-Contract Authority

When other Vatix contracts call the market contract, they should:

1. **Outcome Token Contract** (minting):
   - Calls internal settlement logic (not public entry point)
   - No auth check needed (same deployment)

2. **Treasury Contract** (fee routing):
   - Queries market state (permission-less)
   - No auth needed

3. **Resolution Contract** (adapter coordination):
   - Calls `enable_oracle_adapters()` (admin-gated; must be called by authorized account)
   - Future adapter resolution calls will verify signatures

## Security Considerations

### No Reentrancy Risk
The market contract uses transfer-first-then-update pattern for collateral deposits:
```rust
collateral_token.transfer(&user, &env.current_contract_address(), &amount)?;
// Only after successful transfer:
storage::set_position(&env, market_id, &user, &new_position);
```

### No Overflow/Underflow Risk
Position tracking uses signed integers with validation:
```rust
if position.yes_shares < 0 || position.no_shares < 0 {
    return Err(ContractError::InvalidPosition);
}
```

### Oracle Key Validation
Zero key is explicitly rejected (can never produce valid signature):
```rust
if oracle_pubkey == BytesN::from_array(&env, &[0u8; 32]) {
    return Err(ContractError::InvalidSignature);
}
```

---

## References

- Market contract: [contracts/market/src/lib.rs](../../contracts/market/src/lib.rs)
- Error codes: [contracts/market/src/error.rs](../../contracts/market/src/error.rs)
- Security model: [docs/SECURITY.md](../../docs/SECURITY.md)
