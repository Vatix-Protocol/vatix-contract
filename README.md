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

## Current Status

| Area | Status | Notes |
| --- | --- | --- |
| Binary outcome markets | Complete | Yes/No market creation and lifecycle logic are implemented in the market contract. |
| Share minting and trading | In progress | Core contract flows are in place, with continued validation and integration work. |
| Oracle-based resolution | Complete | Ed25519 verification and fail-closed adapter protection are implemented. |
| Fee distribution | Planned | Treasury routing and fee accounting still need fuller implementation. |
| Market expiration and settlement | In progress | Settlement flows are defined and exercised in tests, with further hardening underway. |

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
test`/`cargo check` build with still unwind by default, which is why a few
integration tests (e.g. `tests/close_market_test.rs`) can use
`std::panic::catch_unwind` to assert a call panics; that pattern only works
against the host-run test binary, not a deployed (release) WASM contract.

Smoke-test that a contract's release build still aborts on panic (useful
after touching a crate's `Cargo.toml` or the workspace profile):

```bash
cd contracts/market
RUSTFLAGS="-C panic=abort" cargo build --release --target wasm32v1-none
```

`RUSTFLAGS` is redundant with the workspace profile setting here — it's just
an explicit way to confirm the setting is actually taking effect for a
specific crate/target combination.

### Cargo Feature Flags

The Market contract supports optional features via Cargo feature flags. These features control compilation of optional modules and dependencies.

#### oracle-adapter Feature

The `oracle-adapter` feature enables the oracle adapter module (`contracts/market/src/oracle_adapter.rs`), which provides:

- `OracleAdapter` trait for abstracting over different oracle providers
- `ReflectorAdapter` implementation for the Reflector on-chain oracle
- `PythAdapter` stub for future Pyth Network integration
- `AnyAdapter` enum for runtime dispatch between adapter types

**Status:** Not enabled by default. The feature is gated because the mainnet switch for oracle adapters is not yet implemented (see issue #139).

**Build with oracle-adapter:**
```bash
cd contracts/market
cargo build --features oracle-adapter
```

**Build without oracle-adapter (default):**
```bash
cd contracts/market
cargo build
# or explicitly
cargo build --no-default-features
```

**When to use:**
- **With feature:** When developing or testing oracle adapter functionality, or when the mainnet switch is implemented
- **Without feature:** For standard market contract deployment using the existing Ed25519 oracle verification path

**Documentation:** See [`docs/adr-001-oracle-adapter.md`](docs/adr-001-oracle-adapter.md) for the complete design rationale, implementation details, and comparison of Reflector vs Pyth oracles.

### Workspace compile check

CI runs `cargo check --workspace --all-targets` in one job (`.github/workflows/ci.yml`) so a break in any single crate (`contracts/market`, `contracts/treasury`, `contracts/resolution`, `contracts/outcome-token`) fails the build immediately, even if that crate isn't otherwise touched by the PR. Run it locally before pushing:

```bash
cargo check --workspace --all-targets
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

### Testnet contract registry

[`deployments/testnet.json`](deployments/testnet.json) (schema documented in [`deployments/README.md`](deployments/README.md)) is the single source of truth for testnet contract IDs, network passphrase, and RPC URL. After deploying a contract, record its ID there so `apps/web` and the scripts below can pick it up. Until a real deploy happens, `contractId` fields are empty-string placeholders.

### Testnet smoke test

`scripts/testnet-smoke.sh` performs a **read-only, simulate-only** invocation (`stellar contract invoke --send=no`) against a deployed Market contract on testnet, to confirm it's reachable — no signing or secret key is required.

```bash
pnpm testnet:smoke
# or
bash scripts/testnet-smoke.sh
```

**What it needs:**
- The `stellar` CLI on PATH
- A market contract ID, resolved from (in order): the `MARKET_CONTRACT_ID` env var, or `.contracts.market.contractId` in `deployments/testnet.json`
- RPC URL / network passphrase, resolved from `SOROBAN_RPC_URL` / `NETWORK_PASSPHRASE` env vars, else the registry file, else the default Stellar testnet values (`https://soroban-testnet.stellar.org`, `Test SDF Network ; September 2015`)

If no contract ID is configured, or the `stellar` CLI isn't installed, the script prints a message and exits `0` (guard mode) instead of failing — it's safe to run on a fresh checkout before any testnet deployment exists.

> **Freighter is not needed here.** Freighter (the browser wallet) is only required for *signed* testnet interactions from the web app; this script never signs or submits a transaction.

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

### Regenerating contract bindings

The web app consumes auto-generated TypeScript bindings for each contract, committed under [`apps/web/lib/contracts/`](apps/web/lib/contracts/README.md). Regenerate them locally whenever a contract's public interface changes:

```bash
pnpm build:bindings
```

**Prerequisites:**
- Stellar CLI **v21+** on PATH
- Rust `wasm32-unknown-unknown` and `wasm32v1-none` targets installed

This runs `scripts/generate-bindings.ts`, which builds every contract to WASM and generates fresh TypeScript clients into `apps/web/lib/contracts/`. **Commit the resulting changes** alongside your contract change.

The `frontend` CI job regenerates bindings on every push/PR and fails with `git diff --exit-code` if the committed output under `apps/web/lib/contracts/` differs from what was just generated — so stale bindings will block CI until you run `pnpm build:bindings` and commit the diff.

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

## Property Test Snapshots Policy (#766)

Property test snapshots and regression seeds (`test_snapshots/`, `property_locked_collateral*`) are uncommitted by default per `.gitignore` rules to keep the repository clean.
To regenerate or run property test invariants locally:

```bash
cargo test --test proptest_locked_invariant
```

## Security

Smart contract security is critical. All contracts will undergo:
- Extensive unit testing
- Integration testing
- External audits before mainnet deployment

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for workspace layout, Soroban contract
patterns, and the build/lint/test commands to run before opening a PR. For
broader project information, check out
[vatix-docs](https://github.com/vatix-protocol/vatix-docs).

## License

MIT License

---

Part of the [Vatix Protocol](https://github.com/vatix-protocol)
