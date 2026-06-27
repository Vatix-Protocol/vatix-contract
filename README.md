# Vatix Contracts

Soroban smart contracts for the Vatix prediction market protocol on Stellar.

## Overview

Core smart contracts powering Vatix prediction markets, written in Rust for the Stellar Soroban platform.

## Contracts

| Contract | Crate | Status | Description |
|---|---|---|---|
| **Market** | `contracts/market` | ✅ Implemented | Market creation, position trading, oracle resolution, and settlement |
| **Treasury** | `contracts/treasury` | ✅ Implemented | Protocol fee collection from withdrawal events; admin-controlled fee withdrawal |
| **Outcome Token** | `contracts/outcome-token` | ✅ Implemented | Fungible SAC-compatible tokens representing YES/NO market outcomes |
| **Resolution** | `contracts/resolution` | ✅ Implemented | Standalone oracle-based outcome resolution with dispute window |

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

## Contract Status

| Contract | Crate | Status | Notes |
|---|---|---|---|
| Market | `contracts/market` | ✅ Complete | Trading, deposit, withdraw, settlement |
| Outcome Token | `contracts/outcome-token` | ✅ Complete | Mint/burn YES/NO tokens; callable only by market contract |
| Resolution | `contracts/resolution` | ✅ Complete | Challenge-window lifecycle for oracle candidates |
| Treasury | `contracts/treasury` | ✅ Complete | Fee collection, custody, and admin withdrawal |

## Tech Stack

- **Language**: Rust
- **Platform**: Stellar Soroban
- **Testing**: Soroban SDK test utilities
- **Build**: Cargo

<!-- ## Project Status

🚧 **Early Stage** - Contract architecture and specifications in progress -->

## Planned Functionality

- Binary outcome markets (Yes/No)
- Share minting and trading
- Oracle-based resolution
- Fee distribution
- Market expiration and settlement

## Resolution Lifecycle

The Market Contract still owns the final `resolve_market(market_id, outcome, signature)` state transition. The separate Resolution Contract adds the missing on-chain challenge window that mirrors the backend `ResolutionCandidate` flow:

1. `propose(proposer, market_id, outcome, signature, evidence_uri, challenge_window_seconds)` stores a signed candidate and publishes its `challenge_deadline`.
2. `challenge(challenger, candidate_id, challenge_uri)` can be called until the deadline. A challenged candidate cannot be finalized.
3. `finalize(finalizer, candidate_id)` succeeds only after the challenge window closes and returns the candidate payload.
4. The backend or registered factory then submits the finalized candidate to `MarketContract::resolve_market`, using the stored outcome and oracle signature.

`contracts/resolution` is intentionally a lifecycle and registration layer, not a replacement settlement engine. `initialize(admin, factory, market_contract)` registers the factory/market relationship so off-chain services can discover which resolution contract guards a market deployment.

## Event Catalog

The Market Contract emits the following events for off-chain indexing and tracking:

| Event | Topics | Fields | Description |
|-------|--------|--------|-------------|
| `contract_initialized_event` | `admin` | `initialized_at: u64` | Emitted when the contract is initialized with an admin |
| `market_created_event` | `market_id` | `creator: Address`, `question: String`, `end_time: u64` | Emitted when a new market is created |
| `collateral_deposited_event` | `user`, `market_id` | `amount: i128`, `new_total: i128` | Emitted when a user deposits collateral into a market |
| `collateral_withdrawn_event` | `user`, `market_id` | `amount: i128`, `new_total: i128` | Emitted when a user withdraws collateral from a market |
| `position_updated_event` | `market_id`, `user` | `yes_shares: i128`, `no_shares: i128`, `locked_collateral: i128` | Emitted when a user's position is updated after trading |
| `trade_executed_event` | `market_id`, `user` | `quantity: i128`, `price_bps: i128`, `side_yes: bool`, `executed_at: u64` | Emitted when a user executes a trade (buy or sell) |
| `position_limit_exceeded_event` | `market_id`, `user` | `side_yes: bool` | Emitted when a trade would result in negative shares |
| `market_resolved_event` | `market_id` | `resolver: BytesN<32>`, `outcome: bool`, `resolved_at: u64` | Emitted when a market is resolved with an oracle-signed outcome |
| `position_settled_event` | `market_id`, `user` | `payout: i128`, `settled_at: u64` | Emitted when a user's position is settled and payout is transferred |
| `oracle_signature_verified_event` | `market_id` | `outcome: bool`, `verified_at: u64` | Emitted when an oracle signature is verified during resolution |
| `fee_calculated_event` | `market_id`, `user` | `fee_amount: i128`, `available_after_fee: i128` | Emitted when a fee is calculated during withdrawal |
| `validation_failed_event` | `context` | `error_code: u32` | Emitted when validation fails, recording context and error code |

### Event Indexing

Off-chain indexers can efficiently filter events using the topic indices:

- **By Market**: Subscribe to events with `market_id` topic to track all activity in a specific market
- **By User**: Subscribe to events with `user` topic to track all activity for a specific user
- **By Trade**: Listen for `trade_executed_event` to capture all trades with quantity, price, and side information



Next.js 16 app for prediction-market UI (mock data + Freighter wallet stub).

```bash
pnpm install
pnpm dev          # http://localhost:3002
pnpm build:web
```

## Getting Started

### Contracts

```bash
# Prerequisites: Rust toolchain, Soroban CLI
cd contracts/market && cargo build
cd ../treasury && cargo build
cd ../outcome-token && cargo build
cd ../resolution && cargo build
```

### Contributor issues

Generate **375** onboarding issues (125 per repo) — see [`scripts/issues/README.md`](scripts/issues/README.md).

```bash
pnpm issues:generate
pnpm issues:publish   # requires gh auth
```

## Deployment

### deploy.sh

Deploys the compiled contract to the configured network.

```bash
# Deploy to testnet
bash scripts/deploy.sh
```

> Requires Soroban CLI and a funded testnet account. Set `SOROBAN_NETWORK` and `SOROBAN_ACCOUNT` env vars before running.

### deploy-testnet.sh

Documents the intended testnet deployment workflow. Currently an echo guard — it prints the deployment steps without executing them, so it is safe to run locally or in CI without side effects.

When a real implementation is ready, this script will:
1. Build the contract to WASM (`cargo build --target wasm32-unknown-unknown --release`)
2. Deploy via Soroban CLI (`soroban contract deploy --wasm <path> --network testnet --source <account>`)
3. Capture and log the returned contract ID for downstream use

```bash
# Preview the testnet deployment steps (no on-chain action)
bash scripts/deploy-testnet.sh
```

> To perform a real deployment, set `SOROBAN_NETWORK=testnet` and `SOROBAN_ACCOUNT=<your-funded-account>` before running once the script is fully implemented.

## Development
```bash
# Prerequisites
- Rust toolchain
- Soroban CLI
- Node 20+ and pnpm 8+ (for apps/web and issue scripts)

# Build contracts
cargo build 
```

## Scripts

The `scripts/` directory contains utility scripts for deployment, invocation, and contributor issue generation. Full documentation is in [`scripts/issues/README.md`](scripts/issues/README.md).

### invoke-example.sh

Smoke-tests a deployed contract by invoking one of its functions via the Soroban CLI. Used in CI to verify that the contract binary is callable after deployment.

```bash
CONTRACT_ID=your_contract_id bash scripts/invoke-example.sh
```

> **Note**: Currently an echo guard. Replace with a real `stellar contract invoke` call once the contract is deployed to a target network — see the TODO comment in the script.

---

## Contract Makefile

The `contracts/market/Makefile` provides convenience targets for day-to-day contract work.

| Target  | Description                                          |
|---------|------------------------------------------------------|
| `build` | Compile the contract to WASM (`wasm32-unknown-unknown`) and print the output size |
| `test`  | Run all unit and integration tests (depends on `build`) |
| `fmt`   | Format all Rust source with `cargo fmt --all`        |
| `clean` | Remove build artefacts via `cargo clean`             |

```bash
# From the repo root
cd contracts/market

make           # default — builds the WASM artefact
make test      # build then run the full test suite
make fmt       # auto-format source files
make clean     # wipe target/ directory
```

## Clippy Lints

[Clippy](https://doc.rust-lang.org/clippy/) is Rust's official linter and is enforced in CI. All warnings are treated as hard errors via `-D warnings`, so the build fails if any lint fires.

```bash
# Run from the contract directory
cd contracts/market
cargo clippy -- -D warnings
```

To suppress a lint where it is intentionally acceptable, add a targeted attribute in the source rather than weakening the global flag:

```rust
#[allow(clippy::lint_name)]
fn my_function() { ... }
```

The CI step is defined in `.github/workflows/ci.yml` and runs automatically on every push and pull request.

## Security

Smart contract security is critical. All contracts will undergo:
- Extensive unit testing
- Integration testing
- External audits before mainnet deployment

## Contributing

Contribution guidelines coming soon. For now, check out [vatix-docs](https://github.com/vatix-protocol/vatix-docs) for project information.

## License

MIT License

---

Part of the [Vatix Protocol](https://github.com/vatix-protocol)
