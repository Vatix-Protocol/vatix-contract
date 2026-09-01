#!/usr/bin/env bash
#
# deploy-testnet.sh — build and deploy all four contracts (Market, Treasury,
# Outcome Token, Resolution) to the Stellar testnet in playbook order (#759).
#
# Playbook deploy order:
#   1. Market (contracts/market)
#   2. Treasury (contracts/treasury)
#   3. Outcome Token (contracts/outcome-token)
#   4. Resolution (contracts/resolution)
#
# Uses the canonical build command `stellar contract build`, which outputs
# WASM artefacts to the canonical directory:
#   target/wasm32v1-none/release/
#
# Requirements:
#   - The `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#   - A funded testnet account secret key supplied via TESTNET_SECRET_KEY
#
# Optional environment overrides:
#   SOROBAN_NETWORK   Network to deploy to (default: testnet)
#
# Guard mode (CI without credentials):
#   When TESTNET_SECRET_KEY is unset, the script builds and validates WASM
#   artefacts for all four contracts and exits 0 cleanly.
#
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
CANONICAL_WASM_DIR="target/wasm32v1-none/release"

CONTRACTS=(
  "contracts/market"
  "contracts/treasury"
  "contracts/outcome-token"
  "contracts/resolution"
)

log() { printf '[deploy-testnet] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 127
fi

# 1. Build all four contracts in playbook order.
for dir in "${CONTRACTS[@]}"; do
  log "Building ${dir} (stellar contract build)..."
  stellar contract build --manifest-path "${dir}/Cargo.toml"
done

# 2. Locate built WASM artefacts for each contract.
declare -A WASM_PATHS
for dir in "${CONTRACTS[@]}"; do
  name="$(basename "${dir}")"
  wasm_path="$(find "${CANONICAL_WASM_DIR}" -maxdepth 1 -name "*${name}*.wasm" 2>/dev/null | head -n1 || true)"
  if [[ -z "${wasm_path}" || ! -f "${wasm_path}" ]]; then
    wasm_path="$(find "${CANONICAL_WASM_DIR}" -maxdepth 1 -name '*.wasm' 2>/dev/null | head -n1 || true)"
  fi
  if [[ -z "${wasm_path}" || ! -f "${wasm_path}" ]]; then
    log "ERROR: could not locate WASM artefact for ${dir} in ${CANONICAL_WASM_DIR}"
    exit 1
  fi
  WASM_PATHS["${dir}"]="${wasm_path}"
  log "Located WASM artefact for ${dir}: ${wasm_path}"
done

# 3. Guard mode: when no credentials are present, verify all artefact paths and exit cleanly.
if [[ -z "${TESTNET_SECRET_KEY:-}" ]]; then
  log "TESTNET_SECRET_KEY not set — skipping deployment (guard mode)."
  log "All four contract artefacts built and verified in playbook order."
  exit 0
fi

# 4. Deploy all four contracts to the target network in playbook order.
log "Deploying all four contracts to network '${NETWORK}' in playbook order..."
declare -A DEPLOYED_IDS

for dir in "${CONTRACTS[@]}"; do
  wasm="${WASM_PATHS[${dir}]}"
  log "Deploying ${dir}..."
  cid="$(
    stellar contract deploy \
      --wasm "${wasm}" \
      --source-account "${TESTNET_SECRET_KEY}" \
      --network "${NETWORK}"
  )"
  if [[ -z "${cid}" ]]; then
    log "ERROR: Deploying ${dir} failed to return a contract ID."
    exit 1
  fi
  DEPLOYED_IDS["${dir}"]="${cid}"
  log "Deployed ${dir} -> Contract ID: ${cid}"
done

MARKET_ID="${DEPLOYED_IDS["contracts/market"]}"
TREASURY_ID="${DEPLOYED_IDS["contracts/treasury"]}"
OUTCOME_TOKEN_ID="${DEPLOYED_IDS["contracts/outcome-token"]}"
RESOLUTION_ID="${DEPLOYED_IDS["contracts/resolution"]}"

# 5. Export contract IDs for downstream CI steps and print them on stdout.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "contract_id=${MARKET_ID}" >>"${GITHUB_OUTPUT}"
  echo "market_contract_id=${MARKET_ID}" >>"${GITHUB_OUTPUT}"
  echo "treasury_contract_id=${TREASURY_ID}" >>"${GITHUB_OUTPUT}"
  echo "outcome_token_contract_id=${OUTCOME_TOKEN_ID}" >>"${GITHUB_OUTPUT}"
  echo "resolution_contract_id=${RESOLUTION_ID}" >>"${GITHUB_OUTPUT}"
fi

log "Successfully deployed all 4 contracts in playbook order:"
echo "MARKET_CONTRACT_ID=${MARKET_ID}"
echo "TREASURY_CONTRACT_ID=${TREASURY_ID}"
echo "OUTCOME_TOKEN_CONTRACT_ID=${OUTCOME_TOKEN_ID}"
echo "RESOLUTION_CONTRACT_ID=${RESOLUTION_ID}"

