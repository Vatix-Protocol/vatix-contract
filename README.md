# Vatix Contracts

Soroban smart contracts for the Vatix prediction market protocol on Stellar.

## Overview

Core smart contracts powering Vatix prediction markets, written in Rust for the Stellar Soroban platform.

## Contracts

| Contract | Crate | Status | Description |
|---|---|---|---|
| **Market** | `contracts/market` | ✅ Implemented | Market creation, position trading, oracle resolution, and settlement |
| **Treasury** | `contracts/treasury` | ✅ Implemented | Protocol fee collection from withdrawal events; admin-controlled fee withdrawal |
| **Outcome Token** | `contracts/outcome-token` | 🚧 Planned | Fungible SAC-compatible tokens representing YES/NO market outcomes |
| **Resolution** | `contracts/resolution` | 🚧 Planned | Standalone oracle-based outcome resolution with dispute window |

### Treasury → Market integration

The Treasury contract is linked to the Market contract via `set_treasury_contract`. Once registered:

1. Every `withdraw_unused_collateral` call deducts a 0.5% protocol fee (`FEE_BPS = 50`).
2. The fee is forwarded to the Treasury contract via a cross-contract `collect_fee` call.
3. The Treasury emits a `FeeCollected` event and accumulates a running total.
4. The admin can drain accumulated fees at any time via `withdraw_fees`.

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

## Frontend (`apps/web`)

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