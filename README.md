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

### Contributor issues

Generate **375** onboarding issues (125 per repo) — see [`scripts/issues/README.md`](scripts/issues/README.md).

```bash
pnpm issues:generate
pnpm issues:publish   # requires gh auth
```

## Deployment

All deployment scripts use the canonical `stellar contract build` command to ensure artifact consistency.

### deploy-testnet.sh

Builds and deploys the contract to Stellar testnet using the unified build toolchain.

**What it does:**
1. Builds the contract using `stellar contract build` (same as Makefile and CI)
2. Locates the WASM artifact at `target/wasm32v1-none/release/*.wasm`
3. Deploys via `stellar contract deploy --wasm <path> --network testnet`
4. Outputs the contract ID for downstream use

```bash
# Set credentials
export TESTNET_SECRET_KEY="S..."

# Deploy to testnet (uses stellar contract build internally)
bash scripts/deploy-testnet.sh
```

**Environment variables:**
- `TESTNET_SECRET_KEY` (required) - Funded testnet account secret key
- `SOROBAN_NETWORK` (optional) - Network name (default: `testnet`)
- `CONTRACT_DIR` (optional) - Contract to build/deploy (default: `contracts/market`)
- `WASM_PATH` (optional) - Explicit WASM path override

### deploy.sh

Generic deployment script for any configured network.

```bash
# Deploy to testnet
bash scripts/deploy.sh
```

> Requires Stellar CLI and a funded account. Set `SOROBAN_NETWORK` and `SOROBAN_ACCOUNT` env vars before running.

### Build Verification

To verify your local WASM matches what CI produces, compare hashes:

```bash
# Build locally
cd contracts/market
stellar contract build

# Compute and verify hash
bash ../../scripts/verify-wasm-hash.sh contracts/market

# Or use the Makefile target
make verify
```

The script outputs the SHA256 hash of the WASM artifact. Compare this with:
- CI build artifacts (download from GitHub Actions)
- Builds from other developers
- Previously deployed contract hashes

Identical hashes confirm the build is reproducible across environments.

### Why Build Consistency Matters

**Artifact mismatch risks:**
- ❌ Local testing with one WASM, deploying another
- ❌ CI tests passing but deployed contract failing
- ❌ Inability to reproduce production builds

**With unified `stellar contract build`:**
- ✅ Same WASM locally, in CI, and deployed
- ✅ Reproducible builds across environments
- ✅ Confidence that tested code is deployed code

## Development

### Build System

**Unified Build Command**: All contracts use `stellar contract build` as the canonical build command.

This ensures:
- ✅ Identical artifacts across local builds, CI, and deployments
- ✅ Optimized WASM for Soroban runtime
- ✅ No drift between development and production builds

```bash
# Build any contract
cd contracts/market
stellar contract build

# Or use the Makefile convenience target
make build
```

The Makefile, CI workflow (`.github/workflows/ci.yml`), and deployment scripts (`scripts/deploy-testnet.sh`) all use this same command to guarantee artifact consistency.

### WASM Artifact Path

All builds output to:
```
target/wasm32v1-none/release/<contract-name>.wasm
```

Example artifacts:
- `vatix_market_contract.wasm`
- `vatix_treasury_contract.wasm`
- `vatix_outcome_token_contract.wasm`
- `vatix_resolution_contract.wasm`

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
| `build` | **Canonical build**: Compiles using `stellar contract build` to produce optimized WASM |
| `test`  | Run all unit and integration tests (depends on `build`) |
| `fmt`   | Format all Rust source with `cargo fmt --all`        |
| `clean` | Remove build artefacts via `cargo clean`             |

```bash
# From the repo root
cd contracts/market

make           # default — builds WASM using stellar contract build
make test      # build then run the full test suite
make fmt       # auto-format source files
make clean     # wipe target/ directory
```

### Build Consistency

The `build` target uses `stellar contract build`, which is the **same command** used by:
- CI pipeline (`.github/workflows/ci.yml`)
- Deployment scripts (`scripts/deploy-testnet.sh`)
- TypeScript bindings generation (`scripts/generate-bindings.ts`)

This unified approach prevents artifact mismatches and ensures the WASM built locally is byte-for-byte identical to what's deployed and tested in CI.

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
