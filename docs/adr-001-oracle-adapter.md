# ADR-001: Soroban Oracle Adapter Interface

**Status:** Proposed  
**Date:** 2026-06-20  
**Issue:** [#139](https://github.com/Vatix-Protocol/vatix-contract/issues/139)

---

## Context

`MarketContract` currently resolves markets via a single Ed25519 keypair stored
in `Market.oracle_pubkey`.  Any market is permanently unresolvable if that key is
lost or compromised, and resolution trust is fully centralised in whoever holds
the private key.

The goal of this spike is to:

1. Define a Soroban-compatible `OracleAdapter` trait (see
   `contracts/market/src/oracle_adapter.rs`).
2. Compare Reflector and Pyth as concrete adapter targets on Soroban testnet.
3. Recommend one for the first real implementation.

---

## Decision Drivers

| Driver | Weight |
|---|---|
| Removes single-key centralization | High |
| Soroban testnet availability today | High |
| Integration complexity / audit surface | Medium |
| Asset coverage | Medium |
| On-chain gas cost | Low (prediction markets are low-frequency) |

---

## Options Considered

### Option A — Keep single Ed25519 signer (status quo)

Retain `oracle_pubkey` per market.  Optionally rotate to a multisig key
off-chain (e.g., a 3-of-5 Schnorr threshold key) before submitting.

**Pros:** No contract changes needed.  
**Cons:** Centralization risk unchanged from the contract's perspective; no
on-chain accountability for the signer set.

---

### Option B — Reflector

[Reflector](https://reflector.network) is a Stellar-native, federated price
oracle.  A network of validators (currently seven independent nodes) signs
each price update via a threshold Ed25519 multisig scheme.  The aggregated
signature is verified inside the Reflector contract, exposing a simple
`lastprice(asset) → {price: i128, timestamp: u64}` interface.

**Testnet contract (as of 2026-06-20):**
`CAZP4SMCQX7L6O42AT4GLLRRSFDXPXS7IH7MMHZ52QWUQBFPXFQVMGQ`

**Integration pattern:**

```rust
// pseudo-code — fill in once OracleAdapter is fully wired
let args = (symbol_short!("BTC"), symbol_short!("USD"));
let result: (i128, u64) = env.invoke_contract(&self.contract_id, &symbol_short!("lastprice"), args.into_val(env));
let (price, _ts) = result;
```

**Pros:**
- No cross-chain latency; contract call is synchronous within a Stellar ledger.
- Simpler integration — one cross-contract call, no off-chain keeper required.
- Threshold multisig removes single-key trust at the oracle layer.
- TWAP endpoint available, reducing price-manipulation risk.
- Open-source contracts, audited.

**Cons:**
- Asset coverage limited to ~30 Stellar-ecosystem pairs (XLM, BTC, ETH, USDC
  pairs against USD).
- Validator set smaller than Pyth's publisher network.

---

### Option C — Pyth Network

[Pyth](https://pyth.network) is a cross-chain pull oracle.  Price data is
published off-chain; a *keeper* (or the resolution caller) must submit a
Wormhole VAA to the Pyth Soroban receiver contract before the price can be
read.  The official `pyth-sdk-soroban` crate wraps the VAA verification.

**Testnet receiver contract (Stellar testnet, as of 2026-06-20):**
`HDWN46CTTXDZ5L5SWKQFUU25L5R2L6XNMCPDWP34PZMBVQJMZAPDVSN`

**Integration pattern (two-step):**

```rust
// Step 1 — submit VAA (caller must pass proof bytes from Hermes API)
env.invoke_contract(&self.contract_id, &symbol_short!("upd_feeds"), (vaa_bytes,).into_val(env));

// Step 2 — read verified price
let price: i64 = env.invoke_contract(&self.contract_id, &symbol_short!("get_price"), (price_id,).into_val(env));
```

**Pros:**
- ~500 price feeds across every major chain.
- Publisher set of 100+ institutions — most decentralized of the two.
- Confidence intervals allow the contract to reject low-confidence prices.
- Battle-tested on 50+ chains.

**Cons:**
- Pull model requires an off-chain keeper to submit VAAs; adds infra dependency
  and latency (Wormhole cross-chain message: ~10–30 s).
- VAA verification is the most expensive on-chain step; higher gas per
  resolution compared to a native cross-contract call.
- Larger integration surface (Wormhole bridge, Hermes API, VAA parsing).

---

## Decision

**Recommend Option B (Reflector) for the first real implementation.**

Rationale:

1. **Sufficient asset coverage** — Vatix markets are anchored to Stellar
   (XLM, USDC-on-Stellar, wrapped BTC/ETH).  Reflector covers every pair
   likely to be used at launch.
2. **Synchronous resolution** — No keeper, no VAA, no cross-chain latency.
   A resolution caller simply triggers `resolve_market`; the adapter fetches
   the price in the same ledger.
3. **Threshold multisig satisfies the centralization requirement** — The
   Reflector network's 7-node threshold multisig provides on-chain proof that
   multiple independent parties agreed on the price.
4. **Lower audit surface** — One cross-contract call vs. VAA decoding +
   Wormhole bridge dependency.

Pyth should be revisited if and when Vatix expands to markets referencing
non-Stellar assets (e.g., SOL, MATIC) or requires confidence-interval
gating for high-stakes markets.

---

## Consequences

### Positive
- Centralization risk eliminated: market resolution no longer depends on a
  single private key.
- No new off-chain infrastructure required for resolution.

### Negative / Risks
- The `Market` struct will need a `resolution_price: i128` threshold field
  (the price at which the market resolves YES); this is a storage-breaking
  change and requires a migration plan before mainnet.
- Reflector's asset list is curated; adding a market for an unlisted asset
  would require a fallback (Ed25519 or Pyth).
- Reflector's 7-node validator set is smaller than Pyth's; a future governance
  vote could reduce the threshold.  Monitor validator-set changes.

### Open Questions
- Should `oracle_pubkey` be kept as an optional fallback for markets that
  pre-date the adapter, or deprecated entirely?
- How should `resolution_price` be expressed for non-USD quote currencies?
- Who is responsible for calling `update_price_feeds` if Pyth is later added?
