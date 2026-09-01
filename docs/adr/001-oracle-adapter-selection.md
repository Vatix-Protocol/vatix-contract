# ADR 001: Oracle Adapter Selection for Market Resolution

## Status

**Accepted** - Initial implementation with Ed25519, extension planned for Reflector and Pyth

## Context

The Vatix prediction market protocol requires a reliable and trustworthy mechanism to resolve market outcomes. Markets need to be resolved based on real-world events or data, which means the smart contract must integrate with external data sources (oracles).

### Problem Statement

How should the Vatix Market Contract verify and accept outcome resolutions from oracles, given that different markets may benefit from different oracle architectures?

### Requirements

1. **Trustworthiness**: Oracle data must be cryptographically verifiable
2. **Flexibility**: Different markets may require different oracle types
3. **Decentralization**: Minimize single points of failure
4. **Cost**: Oracle integration should be gas-efficient
5. **Extensibility**: Should support future oracle types without breaking changes
6. **Security**: Prevent unauthorized or invalid resolutions
7. **Transparency**: Oracle verification logic should be auditable

### Available Oracle Options

#### Option 1: Single Trusted Signer (Ed25519)
- **Description**: Market creator specifies a trusted Ed25519 public key at market creation. Resolution requires a signature from that key.
- **Pros**: 
  - Simple to implement and test
  - Low gas cost
  - Deterministic message format
  - Clear authorization model
- **Cons**: 
  - Centralized (single point of failure)
  - Trusted setup required
  - No redundancy

#### Option 2: Stellar Reflector Oracle
- **Description**: Integration with Stellar's Reflector oracle network for price feeds and real-world data
- **Pros**:
  - Decentralized oracle network
  - Battle-tested on Stellar
  - Multiple data sources
  - Native Soroban integration
- **Cons**:
  - Higher gas cost
  - Limited to data types Reflector supports
  - May have latency
  - Requires contract-to-contract calls

#### Option 3: Pyth Network
- **Description**: Integration with Pyth Network for high-frequency price data
- **Pros**:
  - Extensive price feed coverage
  - High update frequency
  - Multiple publisher verification
  - Cross-chain support
- **Cons**:
  - Higher complexity
  - Gas costs for on-chain verification
  - May require pull-based updates
  - Requires Soroban adapter

#### Option 4: Multi-Signature Threshold
- **Description**: Require N-of-M signatures from multiple trusted signers
- **Pros**:
  - Increased decentralization
  - Fault tolerance (some signers can be offline)
  - No single point of failure
  - Configurable quorum
- **Cons**:
  - More complex verification
  - Higher gas costs
  - Coordination overhead

## Decision

We will implement **a pluggable adapter system** that initially supports Ed25519 signatures with extensibility for Reflector and Pyth adapters.

### Architecture

```rust
pub enum AdapterType {
    Ed25519,      // Single trusted signer (MVP)
    Reflector,    // Stellar Reflector oracle (future)
    Pyth,         // Pyth Network oracle (future)
}

pub struct Market {
    // ... other fields ...
    adapter_type: AdapterType,  // Selected at market creation
    oracle_pubkey: BytesN<32>,  // For Ed25519; repurposed for other adapters
}
```

### Verification Flow

```rust
pub fn verify_market_outcome(
    env: &Env,
    market_id: u32,
    market: &Market,
    adapter_type: AdapterType,
    outcome: bool,
    proof: &BytesN<64>,
) -> Result<(), ContractError> {
    match adapter_type {
        AdapterType::Ed25519 => {
            // Verify Ed25519 signature against oracle_pubkey
            verify_oracle_signature(env, market_id, outcome, proof, &market.oracle_pubkey)
        },
        AdapterType::Reflector => {
            // Future: Query Reflector contract for data
            // Verify proof against Reflector's response
            Err(ContractError::UnauthorizedOracle)  // Not yet implemented
        },
        AdapterType::Pyth => {
            // Future: Verify Pyth price attestation
            // Check price against market conditions
            Err(ContractError::UnauthorizedOracle)  // Not yet implemented
        },
    }
}
```

### Ed25519 Message Format

For Ed25519 adapter, the oracle must sign:

```
message = keccak256(market_id_be || outcome_byte)
```

Where:
- `market_id_be`: 4-byte big-endian u32
- `outcome_byte`: `0x01` for YES, `0x00` for NO

This format is:
- **Deterministic**: Same inputs always produce same message
- **Minimal**: Only 5 bytes of raw data
- **Unambiguous**: Cannot be confused with other message types
- **Efficient**: Fast to construct and verify

### Multi-Signature Extension

To address centralization concerns while using Ed25519, we also support threshold signatures:

```rust
pub fn verify_threshold_signatures(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signers: &Vec<BytesN<32>>,
    signatures: &Vec<BytesN<64>>,
    quorum: u32,
) -> Result<(), ContractError>
```

This allows markets to require M-of-N signatures from a pre-configured signer set.

## Consequences

### Positive

1. **Immediate Functionality**: Ed25519 provides working oracle integration now
2. **Extensibility**: `AdapterType` enum allows adding new adapters without breaking changes
3. **Flexibility**: Market creators can choose the appropriate oracle type
4. **Clear Interface**: `verify_market_outcome` provides uniform verification regardless of adapter
5. **Security**: Each adapter can implement appropriate verification for its data source
6. **Testability**: Simple Ed25519 adapter is easy to test, validate, and reason about
7. **Progressive Enhancement**: Can add Reflector/Pyth support when needed without protocol migration

### Negative

1. **Initial Centralization**: Ed25519 adapter relies on single trusted signer
2. **Incomplete Implementation**: Reflector and Pyth adapters not yet implemented
3. **Field Overloading**: `oracle_pubkey` field serves different purposes for different adapters
4. **Migration Complexity**: Transitioning markets from Ed25519 to decentralized oracles requires coordination
5. **Testing Gap**: Future adapters will require different test infrastructure

### Neutral

1. **Adapter Selection is Permanent**: Markets cannot change adapter type after creation (by design, for security)
2. **Gas Costs Vary**: Different adapters will have different gas costs
3. **Data Availability**: Reflector/Pyth adapters limited to data available from those networks

## Implementation Plan

### Phase 1: Ed25519 Adapter (MVP) ✅ **COMPLETE**

**Status:** Implemented and tested

**Components:**
- `AdapterType` enum with three variants
- `verify_oracle_signature()` for Ed25519
- `verify_market_outcome()` dispatch function
- `construct_oracle_message()` with canonical format
- `verify_threshold_signatures()` for multi-sig support
- Comprehensive test suite (20+ tests)
- Test vector generation for backend alignment

**Security Measures:**
- Zero pubkey rejection
- Safe signature verification (no host traps)
- Deterministic message construction
- Keccak256 hashing for message integrity

### Phase 2: Reflector Adapter (Planned)

**Target:** Q1 2027

**Requirements:**
- Reflector contract integration on Soroban
- Price feed query mechanism
- Timestamp validation
- Staleness checks
- Fallback handling for unavailable data

**Implementation Checklist:**
- [ ] Define Reflector query interface
- [ ] Implement price feed verification
- [ ] Add staleness detection
- [ ] Handle Reflector contract errors
- [ ] Test with live Reflector testnet
- [ ] Document Reflector-specific parameters
- [ ] Update ADR with lessons learned

**Adapter Configuration:**
```rust
// For Reflector markets:
// oracle_pubkey: Address of Reflector contract (reinterpreted as Address)
// Resolution proof: Reflector's signed price attestation
```

### Phase 3: Pyth Adapter (Planned)

**Target:** Q2 2027

**Requirements:**
- Pyth Soroban contract integration
- Price feed verification
- Publisher signature verification
- Confidence interval checking
- Pull-based price update handling

**Implementation Checklist:**
- [ ] Define Pyth query interface
- [ ] Implement price verification
- [ ] Verify publisher signatures
- [ ] Check confidence intervals
- [ ] Test with Pyth testnet
- [ ] Handle price staleness
- [ ] Document Pyth-specific parameters
- [ ] Update ADR with lessons learned

**Adapter Configuration:**
```rust
// For Pyth markets:
// oracle_pubkey: Pyth price feed ID (reinterpreted)
// Resolution proof: Pyth price attestation
```

### Phase 4: Hybrid Adapter (Future)

**Target:** TBD

Allow markets to combine multiple adapters:
- Primary: Reflector price feed
- Fallback: Multi-sig Ed25519 if Reflector unavailable
- Validation: Require both sources to agree within threshold

## Design Rationale

### Why Pluggable Adapters?

**Considered Alternatives:**

1. **Single Adapter Only** (Rejected)
   - Too limiting for diverse market types
   - Cannot handle different data sources
   - No upgrade path without migration

2. **Adapter Registry Pattern** (Rejected for now)
   - Added complexity without immediate benefit
   - Harder to reason about security
   - Can add later if needed

3. **Hardcoded Multiple Adapters** (Rejected)
   - Inflexible for future oracle types
   - Larger contract code size
   - Testing complexity

**Why Start with Ed25519?**

1. **Proven Technology**: Ed25519 is well-tested and secure
2. **Simplicity**: Easiest to implement correctly
3. **Low Cost**: Minimal gas for verification
4. **Testing**: Simple to write comprehensive tests
5. **Debuggability**: Easy to trace and verify
6. **Progressive Enhancement**: Can add decentralized options later

### Why Allow Multiple Adapter Types?

1. **Different Market Needs**: 
   - Price markets → Reflector/Pyth
   - Event markets → Ed25519 trusted reporter
   - High-value markets → Multi-sig threshold

2. **Risk Management**: 
   - Low-value markets can use simpler oracles
   - High-value markets can use more expensive decentralized oracles

3. **Cost Optimization**:
   - Not all markets need expensive decentralized oracles
   - Let market creators optimize cost vs. decentralization

### Why Not Oracle Registry?

We considered a registry where markets lookup oracle addresses at resolution time, but rejected it because:

1. **Security**: Immutable oracle selection is more secure
2. **Predictability**: Users know oracle at market creation
3. **Simplicity**: Fewer moving parts
4. **Gas Cost**: No extra lookup needed

## Security Considerations

### Attack Vectors and Mitigations

#### 1. Unauthorized Resolution

**Attack:** Attacker tries to resolve market with fake signature

**Mitigation:**
- Signature verification against known pubkey
- Zero pubkey rejection
- Message includes market_id to prevent replay
- Keccak256 prevents collision attacks

#### 2. Signature Replay

**Attack:** Valid signature from one market replayed on another

**Mitigation:**
- Message includes market_id
- Each market has unique oracle_pubkey
- Signature only valid for specific (market_id, outcome) pair

#### 3. Oracle Key Compromise

**Attack:** Oracle private key is stolen or leaked

**Mitigation:**
- Immediate: No on-chain solution (trusted setup)
- Future: Multi-sig threshold reduces single-point-of-failure
- Future: Decentralized oracles (Reflector/Pyth) have no single key

#### 4. Oracle Unavailability

**Attack:** Oracle offline or refuses to sign

**Mitigation:**
- Market expiry mechanism
- Cancellation by admin if oracle fails
- Future: Fallback to secondary oracle
- Multi-sig allows M-of-N (some can be offline)

#### 5. Signature Malleability

**Attack:** Attacker modifies signature to create valid variant

**Mitigation:**
- Ed25519 signatures are non-malleable
- Canonical encoding required
- Full signature verification

#### 6. Host Trap DoS

**Attack:** Invalid signature causes host trap, blocking resolution

**Mitigation:**
- Use `verify_ed25519_safe()` with ed25519-dalek
- Returns Result instead of trapping
- Invalid signatures return typed error

### Security Properties

**Guaranteed:**
- ✅ Only authorized oracle can resolve (signature verification)
- ✅ Signature cannot be reused across markets (market_id in message)
- ✅ Invalid signatures cannot trap host (safe verification)
- ✅ Message format is deterministic (keccak256)

**Not Guaranteed (Accept Risk):**
- ❌ Oracle availability (mitigation: market cancellation)
- ❌ Oracle honesty (mitigation: reputation, future multi-sig)
- ❌ Oracle key security (mitigation: future decentralized oracles)

## Testing Strategy

### Unit Tests (20+ tests implemented)

**Message Construction:**
- Deterministic message generation
- Different outcomes produce different messages
- Different market IDs produce different messages
- Edge cases (market_id = 0, market_id = MAX)

**Signature Verification:**
- Valid signatures accept
- Invalid signatures reject
- Wrong outcome rejects
- Wrong market_id rejects
- Different keypair rejects
- Zero pubkey rejects

**Threshold Signatures:**
- M-of-N quorum met accepts
- Below quorum rejects
- Empty signers rejects
- Zero quorum rejects
- Wrong outcome signatures don't count

### Integration Tests

**Market Resolution Flow:**
- Create market with Ed25519 adapter
- Generate valid oracle signature
- Resolve market
- Verify resolution recorded

**Error Paths:**
- Invalid signature fails resolution
- Wrong oracle pubkey fails
- Unauthorized oracle fails

### Test Vectors

**Oracle Message Test Vector:**
- Deterministic seed for reproducibility
- Canonical message format documented
- Backend can validate implementation
- Exported to `test-vectors/oracle-message.json`

## Monitoring and Observability

### Events

**Oracle Verification Events:**
```rust
// Emitted after successful signature verification
event oracle_signature_verified {
    market_id: u32,
    outcome: bool,
    verified_at: u64,
}
```

### Metrics to Track

1. **Resolution Success Rate**: Percentage of markets successfully resolved
2. **Adapter Usage**: Distribution of markets across adapter types
3. **Verification Failures**: Count of invalid signature attempts
4. **Resolution Latency**: Time from market end to resolution

### Failure Scenarios

**Ed25519 Signature Failure:**
- Log: `oracle_pubkey`, `market_id`, `outcome`
- Error: `ContractError::InvalidSignature`
- User Action: Contact oracle operator

**Oracle Unavailable:**
- Admin can cancel market
- Users reclaim collateral
- Log for post-mortem

## Migration Path

### From Ed25519 to Decentralized Oracle

**Not Supported:** Markets cannot change adapter after creation (by design)

**For New Markets:**
1. Deploy Reflector/Pyth adapter implementation
2. Test on testnet thoroughly
3. Update market creation UI to offer new adapter
4. Document gas cost differences
5. Provide guidance on adapter selection

### Backward Compatibility

**Guaranteed:**
- Existing Ed25519 markets continue to work
- `AdapterType` enum is extensible
- `verify_market_outcome` signature remains stable

**Breaking Changes:**
- None planned for existing markets
- New adapters are additive only

## References

### Internal

- Issue #139: Decentralized Oracle Integration
- Issue #368: ADR for Oracle Adapter Selection
- Issue #378: Multi-Signer Threshold Resolution
- `contracts/market/src/oracle.rs`: Implementation
- `contracts/market/src/types.rs`: AdapterType definition
- `test-vectors/oracle-message.json`: Test vector

### External

- [Ed25519 Specification](https://ed25519.cr.yp.to/)
- [Stellar Reflector Oracle](https://github.com/reflector-network)
- [Pyth Network Documentation](https://docs.pyth.network/)
- [Soroban Oracle Patterns](https://developers.stellar.org/docs/smart-contracts)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2024-Q2 | Start with Ed25519 only | MVP needs simple, working oracle |
| 2024-Q3 | Add AdapterType enum | Prepare for future extensions |
| 2024-Q4 | Add threshold signature support | Increase decentralization without full rewrite |
| 2026-06-29 | Document in ADR | Formalize architecture decisions |
| TBD | Add Reflector adapter | When Soroban integration mature |
| TBD | Add Pyth adapter | When Pyth Soroban support available |

## Acceptance Criteria

This ADR is accepted when:

- [x] Document created and reviewed
- [x] Ed25519 adapter fully implemented and tested
- [x] `AdapterType` enum includes all planned types
- [x] `verify_market_outcome` dispatch function implemented
- [x] Threshold signature support implemented
- [x] Test vector generated for backend alignment
- [x] Security considerations documented
- [x] Future adapter interfaces sketched
- [ ] Team consensus on approach (pending PR review)
- [ ] Documentation merged to main branch

## Future Work

### Short Term (Next 6 Months)

1. **Governance Oracle**: Admin-controlled oracle registry for trusted signers
2. **Oracle Reputation**: Track resolution accuracy and response time
3. **Dispute Mechanism**: Allow challenging oracle resolutions within window

### Medium Term (6-12 Months)

1. **Reflector Integration**: Implement Reflector adapter for price feeds
2. **Hybrid Adapter**: Combine multiple oracle sources with fallbacks
3. **Oracle Marketplace**: Let market creators choose from approved oracles

### Long Term (12+ Months)

1. **Pyth Integration**: High-frequency price data for advanced markets
2. **Custom Adapters**: Allow deploying custom oracle adapter contracts
3. **Cross-Chain Oracles**: Oracle data from other chains via bridges

---

**Document Version:** 1.0.0  
**Last Updated:** 2026-06-29  
**Status:** Accepted  
**Authors:** Vatix Protocol Team  
**Reviewers:** TBD
