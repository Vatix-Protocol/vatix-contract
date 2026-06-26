#!/usr/bin/env bash
#
# deploy-testnet.sh — build the market contract and deploy it to the Stellar
# testnet, printing (and exporting) the resulting contract ID.
#
# This replaces the previous echo guard with a real build + deploy flow that
# mirrors `contracts/market/Makefile` (single source of truth for the build:
# `stellar contract build`).
#
# Requirements:
#   - The `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#   - A funded testnet account secret key supplied via TESTNET_SECRET_KEY
#     (in CI this comes from the `TESTNET_SECRET_KEY` repository secret).
#
# Optional environment overrides:
#   SOROBAN_NETWORK   Network to deploy to             (default: testnet)
#   CONTRACT_DIR      Contract crate to build/deploy   (default: contracts/market)
#   WASM_PATH         Explicit path to the built .wasm (default: auto-discovered)
#
# Output:
#   - Prints the deployed contract ID on stdout (last line).
#   - When run inside GitHub Actions, also appends `contract_id=<id>` to
#     $GITHUB_OUTPUT so downstream steps (e.g. the smoke invoke) can consume it.
#
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
CONTRACT_DIR="${CONTRACT_DIR:-contracts/market}"

log() { printf '[deploy-testnet] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 127
fi

if [[ -z "${TESTNET_SECRET_KEY:-}" ]]; then
  log "ERROR: TESTNET_SECRET_KEY is not set."
  log "Provide a funded testnet account secret key (S...) via the"
  log "TESTNET_SECRET_KEY environment variable / repository secret."
  exit 1
fi

# 1. Build the contract WASM using the canonical Makefile build approach.
log "Building ${CONTRACT_DIR} (stellar contract build)..."
stellar contract build --manifest-path "${CONTRACT_DIR}/Cargo.toml"

# 2. Locate the build artefact. `stellar contract build` emits to
#    target/wasm32v1-none/release/. Allow an explicit override for flexibility.
if [[ -z "${WASM_PATH:-}" ]]; then
  WASM_PATH="$(find target/wasm32v1-none/release -maxdepth 1 -name '*.wasm' 2>/dev/null | head -n1 || true)"
fi
if [[ -z "${WASM_PATH:-}" || ! -f "${WASM_PATH}" ]]; then
  log "ERROR: could not locate a built .wasm artefact."
  log "Looked in target/wasm32v1-none/release/. Set WASM_PATH to override."
  exit 1
fi
log "Using WASM artefact: ${WASM_PATH}"

# 3. Deploy to the target network. The contract ID is printed to stdout.
log "Deploying to network '${NETWORK}'..."
CONTRACT_ID="$(
  stellar contract deploy \
    --wasm "${WASM_PATH}" \
    --source-account "${TESTNET_SECRET_KEY}" \
    --network "${NETWORK}"
)"

if [[ -z "${CONTRACT_ID}" ]]; then
  log "ERROR: deploy did not return a contract ID."
  exit 1
fi

log "Deployed contract ID: ${CONTRACT_ID}"

# 4. Export the contract ID for downstream CI steps and print it on stdout.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "contract_id=${CONTRACT_ID}" >>"${GITHUB_OUTPUT}"
fi
echo "${CONTRACT_ID}"
