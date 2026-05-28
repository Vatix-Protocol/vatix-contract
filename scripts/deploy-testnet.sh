#!/usr/bin/env bash
# deploy-testnet.sh
# Guard: echoes intent only. Real implementation should:
#   1. Build the contract (cargo build --target wasm32-unknown-unknown --release)
#   2. Deploy via Soroban CLI (soroban contract deploy --wasm <path> --network testnet --source <account>)
#   3. Capture and log the returned contract ID
echo "Deploying to testnet..."
