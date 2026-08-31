# Market Metadata URI Field - Feature Specification

**Status**: Proposed | Design Phase  
**Feature**: Add optional metadata URI to Market struct  
**Date**: June 30, 2026  

---

## Overview

This feature adds an optional `metadata_uri` field to the Market struct, enabling markets to reference off-chain metadata stored on IPFS, Arweave, or HTTP endpoints.

## Motivation

Currently, markets only store:
- Question (string, 1-499 chars)
- End time (u64)
- Oracle pubkey (32 bytes)
- Collateral token (Address)

This limits the ability to store rich market metadata including:
- Market category/tags
- Detailed description
- Images/logos
- Market rules and terms
- Dispute resolution procedures
- Market creator credentials

The `metadata_uri` field enables:
- Off-chain storage of rich metadata
- Flexible schema (not constrained by on-chain limits)
- Multiple storage backends (IPFS, Arweave, HTTP)
- Reduced on-chain storage costs
- Easy updates via re-deploy (can point to new URI)

## Proposed Design

### Type Change

```rust
#[contracttype]
pub struct Market {
    pub id: u32,
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub status: MarketStatus,
    pub result: Option<bool>,
    pub creator: Address,
    pub created_at: u64,
    pub collateral_token: Address,
    pub price_bps: i128,
    pub resolver: Option<Address>,
    pub resolved_at: Option<u64>,
    pub adapter_type: AdapterType,
    pub outcome_count: u32,
    pub closed_to_deposits: bool,
    
    /// NEW FIELD
    /// Optional URI pointing to off-chain market metadata
    /// Can reference IPFS (ipfs://...), Arweave (ar://...),
    /// or HTTP endpoints. URI is validated to be non-empty
    /// and reasonably sized (max 2048 chars).
    pub metadata_uri: Option<String>,
}
```

### API Changes

#### 1. Create Market with Metadata

```rust
pub fn initialize_market(
    env: Env,
    creator: Address,
    question: String,
    end_time: u64,
    oracle_pubkey: BytesN<32>,
    collateral_token: Address,
    metadata_uri: Option<String>,  // NEW PARAMETER
) -> Result<u32, ContractError>
```

#### 2. Get Market (unchanged)

```rust
pub fn get_market(env: Env, market_id: u32) -> Result<Market, ContractError>
```

Returns market with metadata_uri field populated.

#### 3. Update Market Metadata (optional future enhancement)

```rust
pub fn update_market_metadata(
    env: Env,
    admin: Address,
    market_id: u32,
    metadata_uri: Option<String>,
) -> Result<(), ContractError>
```

(This could be added in future if needed)

### Validation Rules

1. **Optional**: Field can be None (no metadata)
2. **Max Length**: 2048 characters
3. **Non-Empty**: If Some, must be non-empty string
4. **Format Hints** (not enforced):
   - IPFS: `ipfs://QmXxx...`
   - Arweave: `ar://txid...`
   - HTTP: `https://example.com/metadata.json`

### Validation Implementation

```rust
pub fn validate_metadata_uri(uri: &Option<String>) -> Result<(), ContractError> {
    if let Some(uri) = uri {
        // Check non-empty
        if uri.len() == 0 {
            return Err(ContractError::InvalidMetadataUri);
        }
        // Check max length
        if uri.len() > 2048 {
            return Err(ContractError::InvalidMetadataUri);
        }
    }
    Ok(())
}
```

### Error Codes

```rust
#[contracterror]
pub enum ContractError {
    // ... existing errors ...
    
    /// Metadata URI is invalid (empty, too long, or malformed)
    InvalidMetadataUri = 7,
}
```

### Events

Add optional metadata_uri to MarketCreatedEvent:

```rust
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCreatedEvent {
    #[topic]
    pub market_id: u32,
    pub creator: Address,
    pub question: String,
    pub end_time: u64,
    pub metadata_uri: Option<String>,  // NEW FIELD
}
```

### Storage Impact

- **Struct Size Change**: +32 bytes per market (Option<String> overhead)
- **Storage Version**: Bump to 5 (from 4)
- **Migration**: Existing v4 markets will fail to deserialize → UpgradeRequired

### Backward Compatibility

**Breaking Change**: YES
- Old v4 deployments cannot read v5 markets
- Requires migration (redeploy + reinitialize)
- Existing markets must be recreated with metadata_uri field

### Implementation Checklist

- [ ] Add metadata_uri field to Market struct (types.rs)
- [ ] Add InvalidMetadataUri error code (error.rs)
- [ ] Implement validate_metadata_uri() (validation.rs)
- [ ] Update initialize_market() to accept metadata_uri (lib.rs, deposit.rs)
- [ ] Update MarketCreatedEvent to include metadata_uri (events.rs)
- [ ] Bump STORAGE_VERSION to 5 (storage.rs)
- [ ] Add tests for metadata_uri validation (test.rs)
- [ ] Add tests for market creation with metadata (test.rs)
- [ ] Add integration tests (tests/metadata_uri_test.rs)
- [ ] Update documentation (README.md, MIGRATION.md)

### Testing Strategy

#### Unit Tests
1. Valid metadata URIs (IPFS, Arweave, HTTP)
2. Empty metadata_uri (None case)
3. Max length boundary (2047, 2048, 2049)
4. Empty string rejection
5. Event emission verification

#### Integration Tests
1. Create market with metadata_uri
2. Create market without metadata_uri
3. Query market and verify metadata_uri
4. Event contains metadata_uri

### Example Usage

#### Create Market with IPFS Metadata

```rust
let metadata_uri = Some(String::from_str(
    &env,
    "ipfs://QmXxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
));

let market_id = client.initialize_market(
    &admin,
    &String::from_str(&env, "Will BTC reach $100k?"),
    &(env.ledger().timestamp() + 86400),
    &oracle_pubkey,
    &usdc_token,
    &metadata_uri,
);
```

#### Create Market Without Metadata

```rust
let market_id = client.initialize_market(
    &admin,
    &String::from_str(&env, "Will BTC reach $100k?"),
    &(env.ledger().timestamp() + 86400),
    &oracle_pubkey,
    &usdc_token,
    &None,  // No metadata
);
```

### Off-Chain Integration

#### Metadata Format

Recommended JSON schema for metadata at URI:

```json
{
  "version": "1.0",
  "market_id": 1,
  "title": "Bitcoin Price Prediction",
  "description": "Will BTC reach $100,000 by end of 2024?",
  "category": "cryptocurrency",
  "tags": ["bitcoin", "price-prediction", "2024"],
  "source": "Vatix Protocol",
  "images": {
    "logo": "ipfs://QmXxx...",
    "banner": "ipfs://QmYyy..."
  },
  "rules": {
    "settlement": "Oracle-signed resolution",
    "dispute_window": "3 days"
  },
  "creator_info": {
    "address": "GBXXXXXX...",
    "credentials": "verified"
  }
}
```

### Future Enhancements

1. **Update Metadata**: Add admin function to update metadata_uri for active markets
2. **Metadata Validation**: Off-chain service validates URI accessibility
3. **Metadata Caching**: Cache off-chain metadata on-chain if needed
4. **Metadata Versioning**: Track metadata URI history
5. **Rich Categories**: Structured category field alongside metadata_uri

### Migration Path

#### From Storage Version 4 → 5

1. Increment STORAGE_VERSION to 5
2. Rebuild: `stellar contract build`
3. Redeploy contract
4. Call initialize(admin)
5. Recreate markets with metadata_uri field

---

## Design Decisions

### Why Option<String> instead of String?

- **Flexibility**: Existing markets may not have metadata
- **Backward Compatibility**: Can add to old markets
- **Optional**: Not every market needs rich metadata

### Why 2048 Character Limit?

- **Reasonable Bound**: Long enough for most URIs
- **URI Standard**: Standard URI length limits are 2048
- **Storage Efficiency**: Prevents abuse/bloat

### Why Not Store Metadata On-Chain?

- **Cost**: Soroban storage is expensive
- **Flexibility**: Can update metadata without redeployment
- **Scalability**: Unbounded metadata can't fit on-chain

### Why Validate in Contract?

- **Security**: Prevent invalid URIs
- **UX**: Clear error messages
- **State**: Ensure data integrity at write time

---

## Testing Coverage

### Test Cases (Planned)

| Test | Purpose | Status |
|------|---------|--------|
| Valid IPFS URI | Accept valid IPFS metadata URI | Planned |
| Valid Arweave URI | Accept valid Arweave metadata URI | Planned |
| Valid HTTP URI | Accept valid HTTP metadata URI | Planned |
| None metadata_uri | Accept markets without metadata | Planned |
| Empty string rejected | Reject empty metadata_uri | Planned |
| Max length accepted | Accept 2048 char URI | Planned |
| Too long rejected | Reject >2048 char URI | Planned |
| Event includes URI | Verify event emission | Planned |
| Integration test | Full market creation flow | Planned |

---

## Documentation Updates

- [ ] Update README.md with metadata_uri feature
- [ ] Update MIGRATION.md with v4 → v5 migration
- [ ] Update API documentation
- [ ] Add metadata URI specification guide
- [ ] Add off-chain integration guide

---

## Deployment Considerations

### Testnet
- Deploy new version
- Test with various metadata URIs
- Verify event emission
- Test with IPFS/Arweave endpoints

### Mainnet
- Plan migration window
- Communicate storage version change
- Provide migration tools
- Monitor for issues

---

## Success Criteria

✅ metadata_uri field added to Market struct  
✅ Optional field (None = no metadata)  
✅ Validation rules enforced (non-empty, max 2048)  
✅ Error handling for invalid URIs  
✅ Event emission includes metadata_uri  
✅ Storage version bumped to 5  
✅ All tests passing (unit + integration)  
✅ Documentation complete  
✅ Off-chain integration guide provided  

---

## Related Issues/PRs

- Previous: `feat/close-market-to-deposits` (storage v3 → v4)
- Follow-up: None yet

---

## Contacts

- Feature Owner: Vatix Protocol
- Implementation Target: TBD
- Review Requested: Dev Team

---

**Document Status**: Design Review  
**Last Updated**: June 30, 2026  
**Branch**: `feat/market-metadata-uri`
