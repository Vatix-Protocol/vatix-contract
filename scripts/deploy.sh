#!/usr/bin/env bash
# deploy.sh — echo guard for the Soroban contract deployment step.
#
# Purpose: verifies the script is reachable and executable in CI without
# performing a real on-chain deployment. Safe to run locally or in CI.
#
# When a real deployment is needed:
#   1. Build the WASM:  cargo build --target wasm32-unknown-unknown --release
#   2. Deploy:          soroban contract deploy --wasm <path> --network testnet --source <account>
#   3. Capture the returned contract ID for downstream use.
#
# Required env vars for a real deployment:
#   SOROBAN_NETWORK  — target network (e.g. testnet)
#   SOROBAN_ACCOUNT  — funded account name configured in the Soroban CLI
echo deploying to testnet...
