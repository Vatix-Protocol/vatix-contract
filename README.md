# Vatix Contracts

Soroban smart contracts for the Vatix prediction market protocol on Stellar.

## Overview

Core smart contracts powering Vatix prediction markets, written in Rust for the Stellar Soroban platform.

## Contracts

- **Market Contract**: Market creation, trading, and settlement logic
- **Outcome Token**: Fungible tokens representing market outcomes
- **Resolution Contract**: Oracle-based outcome resolution
- **Treasury**: Fee collection and protocol management

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