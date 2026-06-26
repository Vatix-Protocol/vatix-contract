# Canonical build/test entrypoints for the market contract.
#
# `build` is the single source of truth for producing the deployable WASM:
# `stellar contract build`. CI (.github/workflows/ci.yml) and the deploy
# scripts (scripts/deploy-testnet.sh) both build through this same command so
# the artefact path (target/wasm32v1-none/release/*.wasm) never drifts between
# local, CI, and deploy.

default: build

all: test

# Run the full unit + integration test suite for the contract.
test: build
	cargo test

# Compile the contract to its deployable WASM binary.
build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

clean:
	cargo clean
