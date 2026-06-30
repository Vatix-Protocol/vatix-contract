# ADR for Oracle Adapter Selection

Resolves #368

## Summary

This PR adds comprehensive Architecture Decision Record (ADR 001) documenting the oracle adapter selection architecture for market resolution. The ADR captures the rationale, design decisions, and implementation plan for the pluggable oracle adapter system.

## Changes

### Documentation Added

1. **ADR 001: Oracle Adapter Selection** (`docs/adr/001-oracle-adapter-selection.md`)
   - 698-line comprehensive ADR documenting oracle architecture
   - Covers Ed25519 (implemented), Reflector (planned), and Pyth (planned) adapters
   - Includes security analysis, testing strategy, and migration path
   - Documents canonical message format and threshold signature support

2. **ADR Index** (`docs/adr/README.md`)
   - Explains ADR process and when to write ADRs
   - Lists all ADRs with status and dates
   - Provides ADR template and contribution guidelines

### Structure Created

```
docs/
└── adr/
    ├── README.md                          # ADR process documentation
    └── 001-oracle-adapter-selection.md    # Oracle adapter ADR
```

## Key Sections in ADR

### 1. Context & Requirements
- Problem statement: How to verify market outcomes from external oracles
- 7 requirements: Trustworthiness, flexibility, decentralization, cost, extensibility, security, transparency
- 4 oracle options evaluated: Ed25519, Reflector, Pyth, Multi-sig threshold

### 2. Decision
- **Pluggable adapter system** with `AdapterType` enum
- Start with Ed25519, extend to Reflector and Pyth later
- Canonical message format: `keccak256(market_id_be || outcome_byte)`
- Safe signature verification using `ed25519-dalek` (no host trap)

### 3. Architecture

```rust
pub enum AdapterType {
    Ed25519,      // Single trusted signer (MVP)
    Reflector,    // Stellar Reflector oracle (future)
    Pyth,         // Pyth Network oracle (future)
}

pub fn verify_market_outcome(
    env: &Env,
    market_id: u32,
    market: &Market,
    adapter_type: AdapterType,
    outcome: bool,
    proof: &BytesN<64>,
) -> Result<(), ContractError> {
    match adapter_type {
        AdapterType::Ed25519 => verify_oracle_signature(...),
        AdapterType::Reflector => Err(ContractError::UnauthorizedOracle),  // Not yet implemented
        AdapterType::Pyth => Err(ContractError::UnauthorizedOracle),       // Not yet implemented
    }
}
```

### 4. Implementation Plan

**Phase 1: Ed25519 Adapter** ✅ (Complete)
- `AdapterType` enum with 3 variants
- `verify_oracle_signature()` for Ed25519
- `verify_market_outcome()` dispatch function
- `construct_oracle_message()` with canonical format
- `verify_threshold_signatures()` for M-of-N quorum
- 20+ comprehensive tests
- Test vector generation for backend alignment

**Phase 2: Reflector Adapter** (Q1 2027)
- Reflector contract integration
- Price feed query and verification
- Staleness detection
- Testnet validation

**Phase 3: Pyth Adapter** (Q2 2027)
- Pyth Soroban integration
- Publisher signature verification
- Confidence interval checking
- Testnet validation

**Phase 4: Hybrid Adapter** (TBD)
- Multi-adapter support with fallbacks
- Cross-validation between sources

### 5. Security Considerations

**Attack Vectors Analyzed:**
1. Unauthorized resolution → Signature verification
2. Signature replay → market_id in message
3. Oracle key compromise → Accept risk, future multi-sig
4. Oracle unavailability → Market cancellation
5. Signature malleability → Ed25519 non-malleable
6. Host trap DoS → `verify_ed25519_safe()`

**Security Properties:**
- ✅ Only authorized oracle can resolve
- ✅ Signatures cannot be reused across markets
- ✅ Invalid signatures cannot trap host
- ✅ Message format is deterministic

### 6. Testing Strategy

**20+ Tests Implemented:**
- Message construction (8 tests): Deterministic, different outcomes/IDs, edge cases
- Signature verification (6 tests): Valid/invalid, wrong outcome/market_id/keypair
- Threshold signatures (6 tests): M-of-N quorum, below quorum, edge cases
- Test vector (1 test): Deterministic vector exported for backend alignment

## Design Rationale

### Why Pluggable Adapters?

**Benefits:**
- Different market types need different oracles (price vs. event markets)
- Cost optimization (simple markets use cheap Ed25519, high-value use decentralized)
- Future-proof (can add new adapters without breaking changes)
- Security (market creators choose appropriate trust model)

**Rejected Alternatives:**
- Single adapter only → Too limiting
- Adapter registry pattern → Unnecessary complexity for MVP
- Hardcoded multiple adapters → Inflexible

### Why Start with Ed25519?

1. **Proven Technology**: Well-tested and secure
2. **Simplicity**: Easiest to implement correctly
3. **Low Cost**: Minimal gas for verification
4. **Testing**: Simple to write comprehensive tests
5. **Progressive Enhancement**: Can add decentralized oracles later

### Why Canonical Message Format?

**Format**: `keccak256(market_id_be || outcome_byte)`

**Properties:**
- **Deterministic**: Same inputs always produce same message
- **Minimal**: Only 5 bytes of raw data
- **Unambiguous**: Cannot be confused with other message types
- **Replay-resistant**: market_id prevents cross-market replay
- **Efficient**: Fast to construct and verify

## Consequences

### Positive
- ✅ Immediate functionality with Ed25519
- ✅ Extensibility for future oracles
- ✅ Flexibility for market creators
- ✅ Clear verification interface
- ✅ Security through signature verification
- ✅ Easy to test and validate
- ✅ Progressive enhancement path

### Negative
- ❌ Initial centralization (Ed25519 single signer)
- ❌ Reflector/Pyth not yet implemented
- ❌ Field overloading (`oracle_pubkey` serves different purposes)
- ❌ Migration complexity for transitioning markets

### Neutral
- ⚖️ Adapter selection permanent after market creation
- ⚖️ Gas costs vary by adapter
- ⚖️ Data availability limited to oracle capabilities

## Testing

### No Code Changes

This PR is **documentation-only** with no code changes:
- No new tests needed (existing 20+ oracle tests cover implementation)
- No build changes
- No CI changes
- Zero regression risk

### Verification

All existing tests pass:
```bash
cd contracts/market
cargo test oracle
```

Output shows 20+ oracle tests passing (message construction, signature verification, threshold signatures).

## Related Issues

- **Issue #139**: Decentralized Oracle Integration (future work)
- **Issue #378**: Multi-Signer Threshold Resolution (implemented)
- **Issue #368**: This ADR (resolved)

## Documentation Standards

### ADR Quality Checklist

- [x] All sections complete with substantial content
- [x] Code examples for key concepts
- [x] Security analysis thorough and honest
- [x] Implementation plan with actionable checklists
- [x] References to code, issues, and external docs
- [x] Alternatives documented with rationale
- [x] Consequences (positive, negative, neutral) identified
- [x] Testing strategy documented
- [x] Migration path described
- [x] Proper markdown formatting

### Best Practices

- Written in present tense (as if decision is happening now)
- Technical concepts explained clearly
- Code snippets match actual implementation
- Honest assessment of tradeoffs
- Links to internal and external references

## Impact

### Immediate Benefits

- **Documentation**: Clear architectural record for team and auditors
- **Onboarding**: New developers understand oracle design
- **Security**: Attack vectors and mitigations documented
- **Planning**: Roadmap for future oracle integrations

### Long-term Value

- **Decision Traceability**: Future team members understand rationale
- **Audit Trail**: Security auditors can review design
- **Extension Guide**: Clear path for Reflector/Pyth adapters
- **Template**: Model for future ADRs

## Future Work

### Short Term (Next 6 Months)
- Governance oracle: Admin-controlled oracle registry
- Oracle reputation: Track accuracy and response time
- Dispute mechanism: Allow challenging resolutions

### Medium Term (6-12 Months)
- Reflector integration: Price feed adapter
- Hybrid adapter: Multiple oracle sources with fallbacks
- Oracle marketplace: Choose from approved oracles

### Long Term (12+ Months)
- Pyth integration: High-frequency price data
- Custom adapters: Deploy custom oracle contracts
- Cross-chain oracles: Oracle data from other chains

## Checklist

- [x] ADR document created with comprehensive content
- [x] ADR index/README created
- [x] All sections filled with substantial detail
- [x] Code examples match existing implementation
- [x] Security analysis complete
- [x] Implementation phases documented
- [x] Test strategy described
- [x] Related issues referenced
- [x] Summary documents created
- [x] No code changes (documentation only)
- [x] Zero regression risk

## Reviewer Notes

### Focus Areas

1. **Completeness**: Does the ADR capture all relevant context and decisions?
2. **Accuracy**: Do code examples match actual implementation?
3. **Clarity**: Are technical concepts explained clearly?
4. **Security**: Is the security analysis thorough and honest?
5. **Future-proofing**: Does the roadmap make sense?

### Questions for Discussion

1. Should we prioritize Reflector or Pyth for Phase 2?
2. Do we need a formal approval process for future ADRs?
3. Should we include ADR reviews in our security audit scope?
4. Is the timeline for Phases 2-3 realistic?

---

**Type**: Documentation  
**Complexity**: Medium  
**Risk**: None (no code changes)  
**Reviewers**: @team (please review and approve)
