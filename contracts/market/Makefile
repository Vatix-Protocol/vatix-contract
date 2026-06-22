default: build

all: test

# TODO: Replace this echo guard with the real test invocation once the test suite
#       is defined. The real implementation should run `cargo test` to execute all
#       unit and integration tests under `contracts/market/src/`.
test: build
	cargo test

# TODO: Replace this echo guard with the real WASM build command once the contract
#       build pipeline is finalized. The real implementation should compile the
#       contract to a `.wasm` binary deployable to the Stellar network.
build:
	stellar contract build
	@ls -l ../../target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

clean:
	cargo clean
