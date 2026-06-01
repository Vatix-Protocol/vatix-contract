#!/usr/bin/env bash
# deploy.sh
# Guard script used in CI to verify the deployment step is reachable and
# executable without performing a real on-chain deployment.
#
# What this step does in CI:
#   - Confirms the script exists and has execute permissions in the pipeline.
#   - Acts as a placeholder for the full Soroban CLI deployment workflow.
#
# What the real implementation should do once testnet credentials are available
# as repository secrets:
#   1. Build the contract WASM:
#        cargo build --target wasm32-unknown-unknown --release
#   2. Deploy via Soroban CLI:
#        soroban contract deploy \
#          --wasm target/wasm32-unknown-unknown/release/<contract>.wasm \
#          --network testnet \
#          --source <funded-account-alias>
#   3. Capture and export the returned contract ID for downstream CI steps
#      (e.g., smoke-testing via scripts/invoke-example.sh).
#
# See scripts/deploy-testnet.sh for the annotated testnet variant and
# scripts/invoke-example.sh for the post-deploy invocation pattern.
echo "deploying to testnet..."
