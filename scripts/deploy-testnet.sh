#!/bin/bash

# Deploy to testnet (echo guard for now)
# Requires SOROBAN_NETWORK and SOROBAN_ACCOUNT env vars

echo "Deploying to testnet..."
echo "1. Build: cd contracts/market && make build (WASM at target/wasm32v1-none/release/vatix_market_contract.wasm)"
echo "2. Deploy: stellar contract deploy --wasm target/wasm32v1-none/release/vatix_market_contract.wasm --network testnet --source \$SOROBAN_ACCOUNT"
