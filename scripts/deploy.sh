#!/bin/bash

# Deploy to testnet
# Requires SOROBAN_NETWORK and SOROBAN_ACCOUNT env vars

echo "Deploying to testnet..."
echo "Building contract via 'make build' (output: target/wasm32v1-none/release/vatix_market_contract.wasm)"