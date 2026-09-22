# Vatix Contracts

Soroban smart contracts for the Vatix prediction market protocol on Stellar.

## Overview

Core smart contracts powering Vatix prediction markets, written in Rust for the Stellar Soroban platform.

## Contracts

| Contract | Crate | Status | Description |
|---|---|---|---|
| **Market** | `contracts/market` | ✅ Complete | Market creation, position trading, oracle resolution, and settlement |
| **Treasury** | `contracts/treasury` | ✅ Complete | Protocol fee collection from withdrawal events; admin-controlled fee withdrawal |
| **Outcome Token** | `contracts/outcome-token` | ✅ Complete | Fungible SAC-compatible tokens representing YES/NO market outcomes |
| **Resolution** | `contracts/resolution` | ✅ Complete | Standalone oracle-based outcome resolution with dispute window |

### Outcome Token SAC metadata

Each Outcome Token contract instance is initialized with SAC-compatible metadata, matching the `OutcomeTokenContract::initialize` / `set_metadata` implementation in `contracts/outcome-token/src/lib.rs`:

| Field | Type | Source | Notes |
|---|---|---|---|
| `name` | `String` | Set at `initialize`, mutable via `set_metadata` (admin only) | Human-readable token name, e.g. `"Vatix YES Token"` |
| `symbol` | `String` | Set at `initialize`, mutable via `set_metadata` (admin only) | Ticker symbol, e.g. `"vYES"` / `"vNO"` |
| `decimals` | `u32` | Compile-time constant, not stored | Fixed at `7`, matching the Stellar Asset Contract (SAC) standard |

`name`, `symbol`, and `decimals` are exposed via the `name()`, `symbol()`, and `decimals()` getters.

### Optional Market integrations

The Market contract can optionally wire supporting modules via admin-configured contract addresses. Once registered:

1. `set_treasury` registers a Treasury contract address that receives fee deposits from `withdraw_unused_collateral`.
2. `set_outcome_token_contract` registers an Outcome Token contract that mints/burns tokens when market positions change.
3. `set_resolution_contract` registers a Resolution contract that gates `resolve_market` until a candidate is finalized.
4. When configured, `withdraw_unused_collateral` computes a fee, transfers it to the Treasury, and records it via `collect_fee`.
- **Market Contract**: Market creation, trading, and settlement logic
- **Treasury**: Fee collection and protocol management
- **Outcome Token**: Mint/burn YES/NO outcome share tokens
- **Resolution Contract**: Challenge-window lifecycle for oracle resolution candidates

See [`docs/cross-contract-call-graph.md`](docs/cross-contract-call-graph.md) for the full edge-by-edge call graph, authorization requirements, and registration prerequisites.

## Tech Stack

- **Language**: Rust
- **Platform**: Stellar Soroban
- **Testing**: Soroban SDK test utilities
- **Build**: Cargo

## Documentation

### Security

Please review our [Security Policy](SECURITY.md) for information on reporting contract vulnerabilities.

### Storage Migrations

The Market contract uses storage versioning to ensure data integrity across upgrades. See comprehensive documentation:

- **[Storage Migration Guide](contracts/market/STORAGE_MIGRATION_GUIDE.md)** - Complete guide for handling storage version bumps, including:
  - [Reviewer checklist for `StorageKey` table drift](contracts/market/STORAGE_MIGRATION_GUIDE.md#reviewer-checklist-storagekey-table-drift) - how to verify the `StorageKey` enum and the `lib.rs` storage table stay in sync
  - When to bump storage version
  - Step-by-step migration procedures (testnet & mainnet)
  - Testing strategies
  - Rollback and recovery procedures
  - Common pitfalls and solutions
  
- **[Migration History](contracts/market/MIGRATION.md)** - Specific changes and data migration notes

- **[Cross-Contract Upgrade Playbook](scripts/upgrade/UPGRADE_PLAYBOOK.md)** - Executable, multi-contract upgrade safety net covering Market, Treasury, Resolution, and Outcome Token together: deploy order, WASM hash pinning, the storage version compatibility matrix, a dual-read migration template for the next storage bump, a staging dry-run checklist, and rollback. Run `bash scripts/upgrade/check-upgrade.sh` for a scripted pass/fail dry-run; see the `upgrade-dry-run` CI job in `.github/workflows/ci.yml` for how it's enforced automatically.

  **Fail-closed hash pinning:** `scripts/upgrade/check-upgrade.sh` is fail-closed for any contract pinned in `expected-hashes.json`. A pinned entry with an empty, missing, or mismatched hash causes a non-zero exit (the dry-run fails) rather than a warning. Unpinned entries (absent from `expected-hashes.json`) remain non-failing. To intentionally pin a contract, add its WASM hash to `expected-hashes.json`; to unpin, remove the entry. Never leave a pinned entry with an empty hash — that is treated as drift and will fail CI.

**Quick Reference:**
- Current storage version: `3`
- Always bump version for: field changes, type changes, semantic changes to stored data
- Migration is required when deploying with a new storage version

### Treasury Storage

- **[Treasury Storage Layout](docs/treasury-storage.md)** - Complete `StorageKey` reference for the Treasury contract, including:
  - Every key, its storage tier (instance vs persistent), value type, and description
  - Storage version history
  - Notes on the version guard and fee token registry

**Quick Reference:**
- Current storage version: `3`
- Always bump version for: field changes, type changes, semantic changes to stored data
- Migration is required when deploying with a new storage version

<!-- ## Project Status

🚧 **Early Stage** - Contract architecture and specifications in progress -->

## Planned Functionality

- Binary outcome markets (Yes/No)
- Share minting and trading
- Oracle-based resolution
- Fee distribution
- Market expiration and settlement

## Recent Features

### Close Market to Deposits (v1.2)

Allows administrators to prevent new collateral deposits into a market while preserving all other functionality (trading, withdrawals, and settlement). 

**Use Cases**:
- Prevent new positions when approaching market expiration
- Lock down markets during resolution or dispute windows
- Manage market liquidity and exposure

**API**: 
```rust
pub fn close_market_to_deposits(env: Env, admin: Address, market_id: u32) -> Result<(), ContractError>
```

**Event**:
```
MarketClosedToDeposits {
    market_id: u32,
    admin: Address,
    closed_at: u64,
}
```

For detailed documentation, see [CLOSE_MARKET_FEATURE.md](CLOSE_MARKET_FEATURE.md).

## Resolution Lifecycle

The Market Contract still owns the final `resolve_market(market_id, outcome, signature)` state transition. The separate Resolution Contract adds the missing on-chain challenge window that mirrors the backend `ResolutionCandidate` flow:

1. `propose(proposer, market_id, outcome, signature, evidence_uri, challenge_window_seconds)` stores a signed candidate and publishes its `challenge_deadline`.
2. `challenge(challenger, candidate_id, challenge_uri)` can be called until the deadline. A challenged candidate cannot be finalized.
3. `finalize(finalizer, candidate_id)` succeeds only after the challenge window closes and returns the candidate payload.
4. The backend or registered factory then submits the finalized candidate to `MarketContract::resolve_market`, using the stored outcome and oracle signature.

`contracts/resolution` is intentionally a lifecycle and registration layer, not a replacement settlement engine. `initialize(admin, factory, market_contract)` registers the factory/market relationship so off-chain services can discover which resolution contract guards a market deployment.

## Event Catalog

> For the complete, up-to-date field reference across **all four contracts** (Market, Treasury, Resolution, Outcome Token), see [`docs/events-reference.md`](docs/events-reference.md) — the canonical schema reference for off-chain indexers. The table below covers a subset of Market events for a quick overview.

The Market Contract emits the following events for off-chain indexing and tracking:

| Event | Topics | Fields | Description |
|-------|--------|--------|-------------|
| `contract_initialized` | `admin` | `initialized_at: u64` | Emitted when the contract is initialized with an admin |
| `market_created` | `market_id` | `creator: Address`, `question: String`, `end_time: u64` | Emitted when a new market is created |
| `collateral_deposited` | `user`, `market_id` | `amount: i128`, `new_total: i128` | Emitted when a user deposits collateral into a market |
| `collateral_withdrawn` | `user`, `market_id` | `amount: i128`, `new_total: i128` | Emitted when a user withdraws collateral from a market |
| `position_updated` | `market_id`, `user` | `yes_shares: i128`, `no_shares: i128`, `locked_collateral: i128` | Emitted when a user's position is updated after trading |
| `trade_executed` | `market_id`, `user` | `quantity: i128`, `price_bps: i128`, `side_yes: bool`, `executed_at: u64` | Emitted when a user executes a trade (buy or sell) |
| `position_limit_exceeded` | `market_id`, `user` | `side_yes: bool` | Emitted when a trade would result in negative shares |
| `market_resolved` | `market_id` | `resolver: BytesN<32>`, `outcome: bool`, `resolved_at: u64` | Emitted when a market is resolved with an oracle-signed outcome |
| `position_settled` | `market_id`, `user` | `payout: i128`, `settled_at: u64` | Emitted when a user's position is settled and payout is transferred |
| `oracle_signature_verified` | `market_id` | `outcome: bool`, `verified_at: u64` | Emitted when an oracle signature is verified during resolution |
| `fee_calculated` | `market_id`, `user` | `fee_amount: i128`, `available_after_fee: i128` | Emitted when a fee is calculated during withdrawal |
| `validation_failed` | `context` | `error_code: u32` | Emitted when validation fails, recording context and error code |

### Event Indexing

Off-chain indexers can efficiently filter events using the topic indices:

- **By Market**: Subscribe to events with `market_id` topic to track all activity in a specific market
- **By User**: Subscribe to events with `user` topic to track all activity for a specific user
- **By Trade**: Listen for `trade_executed` to capture all trades with quantity, price, and side information



Next.js 16 app for prediction-market UI (mock data + Freighter wallet stub).

```bash
pnpm install
pnpm dev          # http://localhost:3002
pnpm build:web
```

> **Freighter wallet integration:** See [`docs/freighter-integration-guide.md`](docs/freighter-integration-guide.md)
> for setup instructions, transaction signing flow, ScVal helpers, and
> troubleshooting — including [network mismatch errors](docs/freighter-integration-guide.md#network-mismatch-errors-issue-587).

## Getting Started

### Prerequisites

- **Rust toolchain** (stable, with `wasm32-unknown-unknown` target)
- **Stellar CLI** (v21.4.0+) - Install from [stellar.org/docs/tools/cli](https://developers.stellar.org/docs/tools/cli)
- **Node 20+** and **pnpm 8+** (for web app and scripts)

```bash
# Install Rust and add WASM target
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Stellar CLI (macOS/Linux)
# See https://developers.stellar.org/docs/tools/cli for other platforms
curl -L https://github.com/stellar/stellar-cli/releases/download/v21.4.0/stellar-cli-21.4.0-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv stellar /usr/local/bin/

# Verify installation
stellar --version
```

### Building Contracts

**All contract builds use the canonical command: `stellar contract build`.**

This is the single source of truth across Makefile, CI, and deployment scripts to ensure byte-for-byte identical WASM artifacts. The output path is always:
```
target/wasm32v1-none/release/<contract-name>.wasm
```

```bash
# Prerequisites: Rust toolchain, Soroban CLI
cd contracts/market && cargo build
cd ../treasury && cargo build
cd ../outcome-token && cargo build
cd ../resolution && cargo build
```

### Panic Strategy (Soroban Contract Builds)

The workspace `[profile.release]` (`Cargo.toml`) sets `panic = "abort"`. WASM
has no stack-unwinding support, so a panicking contract call must abort
(trap the VM) rather than unwind — this is required for `wasm32v1-none`
output and also keeps release binaries smaller.

This only applies to `--release` builds — the same profile `stellar contract
build` and the deploy scripts use. The `dev`/`test` profiles that `cargo
test`/`cargo check` build with still unwind, so `#[should_panic]` tests and
`catch_unwind`-style assertions continue to work in the test suite.
