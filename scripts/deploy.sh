#!/usr/bin/env bash
#
# deploy.sh — build, deploy, and smoke-invoke contract on Stellar network (#760).
# Replaces the previous echo guard with real `stellar contract build`,
# `stellar contract deploy`, and `stellar contract invoke` executions.
#
# Requirements:
#   - `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#   - TESTNET_SECRET_KEY or SECRET_KEY for live deployment and invocation
#
# Environment overrides:
#   SOROBAN_NETWORK   Target network (default: testnet)
#   CONTRACT_DIR      Crate directory to build and deploy (default: contracts/market)
#   SMOKE_FN          Function to invoke as smoke check (default: get_treasury)
#
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
CONTRACT_DIR="${CONTRACT_DIR:-contracts/market}"
SMOKE_FN="${SMOKE_FN:-get_treasury}"
CANONICAL_WASM_DIR="target/wasm32v1-none/release"

log() { printf '[deploy] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 127
fi

log "Building contract at ${CONTRACT_DIR}..."
stellar contract build --manifest-path "${CONTRACT_DIR}/Cargo.toml"

WASM_PATH="$(find "${CANONICAL_WASM_DIR}" -maxdepth 1 -name '*.wasm' 2>/dev/null | head -n1 || true)"
if [[ -z "${WASM_PATH:-}" || ! -f "${WASM_PATH}" ]]; then
  log "ERROR: could not locate built .wasm artefact in ${CANONICAL_WASM_DIR}"
  exit 1
fi
log "Using WASM artefact: ${WASM_PATH}"

SECRET_KEY="${TESTNET_SECRET_KEY:-${SECRET_KEY:-}}"
if [[ -z "${SECRET_KEY}" ]]; then
  log "TESTNET_SECRET_KEY / SECRET_KEY not set — skipping live deployment and invocation (guard mode)."
  log "Artefact build verified: ${WASM_PATH}"
  exit 0
fi

log "Deploying contract to network '${NETWORK}'..."
CONTRACT_ID="$(
  stellar contract deploy \
    --wasm "${WASM_PATH}" \
    --source-account "${SECRET_KEY}" \
    --network "${NETWORK}"
)"

if [[ -z "${CONTRACT_ID}" ]]; then
  log "ERROR: contract deploy failed to return a contract ID."
  exit 1
fi
log "Deployed Contract ID: ${CONTRACT_ID}"

log "Executing smoke check (invoke '${SMOKE_FN}')..."
RESULT="$(
  stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --source-account "${SECRET_KEY}" \
    --network "${NETWORK}" \
    -- "${SMOKE_FN}"
)"

log "Smoke invoke succeeded: ${RESULT:-<void>}"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "contract_id=${CONTRACT_ID}" >>"${GITHUB_OUTPUT}"
fi
echo "${CONTRACT_ID}"

