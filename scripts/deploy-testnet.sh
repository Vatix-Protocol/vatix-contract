#!/usr/bin/env bash
#
# deploy-testnet.sh — build the market contract and deploy it to the Stellar
# testnet, printing (and exporting) the resulting contract ID.
#
# Uses the canonical build command `stellar contract build`, which outputs
# the WASM artefact to the canonical path:
#
#   target/wasm32v1-none/release/<contract-name>.wasm
#
# This path is the single source of truth across Makefile, CI, and deploy
# scripts, ensuring byte-for-byte identical artefacts everywhere.
#
# Requirements:
#   - The `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#   - A funded testnet account secret key supplied via TESTNET_SECRET_KEY
#     (in CI this comes from the `TESTNET_SECRET_KEY` repository secret).
#
# Optional environment overrides:
#   SOROBAN_NETWORK   Network to deploy to             (default: testnet)
#   CONTRACT_DIR      Contract crate to build/deploy   (default: contracts/market)
#   WASM_PATH         Explicit path to the built .wasm (default: auto-discovered
#                     from canonical path target/wasm32v1-none/release/)
#
# Guard mode (CI without credentials):
#   When TESTNET_SECRET_KEY is unset the script validates that the canonical
#   build artefact exists and exits 0 — no deployment is attempted. This lets
#   the CI step act as a reachability + path check without real credentials.
#
# Output:
#   - Prints the deployed contract ID on stdout (last line).
#   - When run inside GitHub Actions, also appends `contract_id=<id>` to
#     $GITHUB_OUTPUT so downstream steps (e.g. the smoke invoke) can consume it.
#
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
CONTRACT_DIR="${CONTRACT_DIR:-contracts/market}"

# Canonical WASM output path produced by `stellar contract build`.
CANONICAL_WASM_DIR="target/wasm32v1-none/release"

log() { printf '[deploy-testnet] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 127
fi

# 1. Build using the canonical command so the artefact lands at
#    target/wasm32v1-none/release/<name>.wasm.
log "Building ${CONTRACT_DIR} (stellar contract build)..."
stellar contract build --manifest-path "${CONTRACT_DIR}/Cargo.toml"

# 2. Locate the artefact at the canonical output path.
#    Allow an explicit WASM_PATH override for flexibility.
if [[ -z "${WASM_PATH:-}" ]]; then
  WASM_PATH="$(find "${CANONICAL_WASM_DIR}" -maxdepth 1 -name '*.wasm' 2>/dev/null | head -n1 || true)"
fi
if [[ -z "${WASM_PATH:-}" || ! -f "${WASM_PATH}" ]]; then
  log "ERROR: could not locate a built .wasm artefact."
  log "Looked in ${CANONICAL_WASM_DIR}/. Set WASM_PATH to override."
  exit 1
fi
log "Using WASM artefact: ${WASM_PATH}"

# 3. Guard mode: when no credentials are present, verify the artefact path and
#    exit cleanly without attempting a live deployment.
if [[ -z "${TESTNET_SECRET_KEY:-}" ]]; then
  log "TESTNET_SECRET_KEY not set — skipping deployment (guard mode)."
  log "Artefact path verified: ${WASM_PATH}"
  exit 0
fi

# 4. Deploy to the target network. The contract ID is printed to stdout.
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

# 5. Export the contract ID for downstream CI steps and print it on stdout.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "contract_id=${CONTRACT_ID}" >>"${GITHUB_OUTPUT}"
fi
echo "${CONTRACT_ID}"
