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

## Getting Started

Coming soon

## Development
```bash
# Prerequisites
- Rust toolchain
- Soroban CLI

# Build
cargo build 
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