# Canonical build/test entrypoints for the market contract.
#
# `build` is the single source of truth for producing the deployable WASM:
# `stellar contract build`. CI (.github/workflows/ci.yml) and the deploy
# scripts (scripts/deploy-testnet.sh) both build through this same command so
# the artefact path (target/wasm32v1-none/release/*.wasm) never drifts between
# local, CI, and deploy.
#
# IMPORTANT: Do not use `cargo build --target wasm32-unknown-unknown` directly.
# That produces different artifacts and breaks consistency guarantees.
# Always use `stellar contract build` or `make build`.

default: build

all: test

# Run the full unit + integration test suite for the contract.
test: build
	cargo test

# Compile the contract to its deployable WASM binary using the canonical command.
# This is the ONLY way contracts should be built for deployment.
# Output: target/wasm32v1-none/release/vatix_market_contract.wasm
build:
	stellar contract build
	@echo "WASM artifact:"
	@ls -lh target/wasm32v1-none/release/*.wasm

# Verify the WASM build is reproducible by displaying its hash.
# Compare this hash across environments to ensure consistency.
verify:
	@bash ../../scripts/verify-wasm-hash.sh contracts/market

fmt:
	cargo fmt --all

clean:
	cargo clean
