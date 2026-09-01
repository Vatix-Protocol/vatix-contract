#!/usr/bin/env bash
#
# testnet-smoke.sh — read-only smoke test against the four Vatix contracts
# (Market, Treasury, Resolution, Outcome Token) already deployed on Stellar
# testnet.
#
# Part 1 (liveness) performs a single simulated (never signed/submitted)
# invocation of a harmless read-only getter (`get_fee_rate` on Market, which
# takes no arguments beyond `env`) to prove the configured Market contract ID
# is reachable and callable on the configured RPC endpoint.
#
# Part 2 (cross-contract wiring, #698) simulates a handful of additional
# read-only getters — one per registered pairing — and checks that each
# contract's configured address for its counterpart actually matches the
# counterpart's own contract ID from the registry. This catches exactly the
# "partial re-wire" failure mode `scripts/upgrade/UPGRADE_PLAYBOOK.md` warns
# about: e.g. Market pointing at a new Resolution deployment while Resolution
# still points at the old Market address.
#
# Nothing is written on-chain and no secret key is required — `stellar
# contract invoke --send=no` only simulates the call, and `--source-account`
# accepts a bare public key (G...) for simulation, so no funded/signing
# account is needed.
#
# Requirements:
#   - The `stellar` CLI on PATH (https://developers.stellar.org/docs/tools/cli)
#
# Contract ID resolution (first match wins, per contract):
#   1. <NAME>_CONTRACT_ID environment variable (MARKET_CONTRACT_ID,
#      TREASURY_CONTRACT_ID, RESOLUTION_CONTRACT_ID,
#      OUTCOME_TOKEN_CONTRACT_ID)
#   2. .contracts.<name>.contractId in deployments/testnet.json (see
#      deployments/README.md for the registry schema)
#
# Optional environment overrides:
#   SOROBAN_RPC_URL      RPC endpoint      (default: from registry, else
#                         https://soroban-testnet.stellar.org)
#   NETWORK_PASSPHRASE   Network passphrase (default: from registry, else
#                         "Test SDF Network ; September 2015")
#   SMOKE_FN             Read-only function to call against Market for the
#                         liveness check (default: get_fee_rate)
#   SMOKE_SOURCE_ACCOUNT Public key used to build the simulated transaction
#                         (default: a fixed placeholder G... address — no
#                         secret key or funding required for a view call)
#
# Guard mode:
#   If the `stellar` CLI is missing, or no Market contract ID is configured
#   via either MARKET_CONTRACT_ID or the registry file, the whole script
#   prints a clear message and exits 0 rather than failing — this script is
#   meant to be safely runnable (e.g. in CI or fresh checkouts) before any
#   real testnet deployment exists.
#
#   Each individual wiring check additionally soft-skips (logs and moves on,
#   never fails the run) whenever either side of that specific pairing's
#   contract ID isn't configured — a partially-deployed environment (e.g.
#   only Market and Treasury live so far) still gets useful signal from the
#   checks it *can* run instead of an all-or-nothing skip.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REGISTRY_FILE="${ROOT_DIR}/deployments/testnet.json"

SMOKE_FN="${SMOKE_FN:-get_fee_rate}"
# Well-known placeholder public key (does not need to exist/be funded — the
# call is simulated only via --send=no, never signed or submitted).
SMOKE_SOURCE_ACCOUNT="${SMOKE_SOURCE_ACCOUNT:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF}"

log() { printf '[testnet-smoke] %s\n' "$*" >&2; }

WIRING_FAILED=0

# ---------------------------------------------------------------------------
# Read a dotted field out of deployments/testnet.json, preferring `jq` and
# falling back to a small inline `node` one-liner. Prints an empty string
# (not an error) if the file, tool, or field is missing.
# ---------------------------------------------------------------------------
read_registry_field() {
  local jq_filter="$1"
  local node_expr="$2"

  if [[ ! -f "${REGISTRY_FILE}" ]]; then
    echo ""
    return 0
  fi

  if command -v jq >/dev/null 2>&1; then
    jq -r "${jq_filter} // empty" "${REGISTRY_FILE}" 2>/dev/null || echo ""
    return 0
  fi

  if command -v node >/dev/null 2>&1; then
    node -e "
      try {
        const fs = require('fs');
        const data = JSON.parse(fs.readFileSync(process.argv[1], 'utf8'));
        const val = (${node_expr});
        if (val !== undefined && val !== null) process.stdout.write(String(val));
      } catch (e) { /* ignore, treat as unset */ }
    " "${REGISTRY_FILE}" 2>/dev/null || echo ""
    return 0
  fi

  # Neither jq nor node available — degrade gracefully to "unset".
  echo ""
}

# 1. Resolve all four contract IDs: env var wins, else registry file.
MARKET_CONTRACT_ID="${MARKET_CONTRACT_ID:-}"
if [[ -z "${MARKET_CONTRACT_ID}" ]]; then
  MARKET_CONTRACT_ID="$(read_registry_field '.contracts.market.contractId' 'data.contracts && data.contracts.market && data.contracts.market.contractId')"
fi

TREASURY_CONTRACT_ID="${TREASURY_CONTRACT_ID:-}"
if [[ -z "${TREASURY_CONTRACT_ID}" ]]; then
  TREASURY_CONTRACT_ID="$(read_registry_field '.contracts.treasury.contractId' 'data.contracts && data.contracts.treasury && data.contracts.treasury.contractId')"
fi

RESOLUTION_CONTRACT_ID="${RESOLUTION_CONTRACT_ID:-}"
if [[ -z "${RESOLUTION_CONTRACT_ID}" ]]; then
  RESOLUTION_CONTRACT_ID="$(read_registry_field '.contracts.resolution.contractId' 'data.contracts && data.contracts.resolution && data.contracts.resolution.contractId')"
fi

OUTCOME_TOKEN_CONTRACT_ID="${OUTCOME_TOKEN_CONTRACT_ID:-}"
if [[ -z "${OUTCOME_TOKEN_CONTRACT_ID}" ]]; then
  OUTCOME_TOKEN_CONTRACT_ID="$(read_registry_field '.contracts.outcomeToken.contractId' 'data.contracts && data.contracts.outcomeToken && data.contracts.outcomeToken.contractId')"
fi

# 2. Resolve RPC URL and network passphrase: env var wins, else registry,
#    else the well-known Stellar testnet defaults.
RPC_URL="${SOROBAN_RPC_URL:-}"
if [[ -z "${RPC_URL}" ]]; then
  RPC_URL="$(read_registry_field '.rpcUrl' 'data.rpcUrl')"
fi
RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"

PASSPHRASE="${NETWORK_PASSPHRASE:-}"
if [[ -z "${PASSPHRASE}" ]]; then
  PASSPHRASE="$(read_registry_field '.networkPassphrase' 'data.networkPassphrase')"
fi
PASSPHRASE="${PASSPHRASE:-Test SDF Network ; September 2015}"

# 3. Guard mode: no Market contract ID configured anywhere. Market is the
#    anchor contract for this whole script (every other contract's wiring
#    check is expressed relative to it), so without it there's nothing
#    meaningful left to run.
if [[ -z "${MARKET_CONTRACT_ID}" ]]; then
  log "No market contract ID configured — skipping smoke test (guard mode)."
  log "Set MARKET_CONTRACT_ID, or fill in .contracts.market.contractId in"
  log "${REGISTRY_FILE#"${ROOT_DIR}/"} (see deployments/README.md)."
  exit 0
fi

# 4. Guard mode: stellar CLI missing.
if ! command -v stellar >/dev/null 2>&1; then
  log "'stellar' CLI not found on PATH — skipping smoke test (guard mode)."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 0
fi

# ---------------------------------------------------------------------------
# invoke <contract-id> <fn> [args...] — simulate a read-only call and print
# its raw result. --send=no simulates the call only; it is never signed or
# submitted, so no secret key or funded account is required.
# ---------------------------------------------------------------------------
invoke() {
  local contract_id="$1" fn="$2"
  shift 2
  stellar contract invoke \
    --id "${contract_id}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${PASSPHRASE}" \
    --source-account "${SMOKE_SOURCE_ACCOUNT}" \
    --send=no \
    -- "${fn}" "$@"
}

log "Smoke-invoking '${SMOKE_FN}' on market contract ${MARKET_CONTRACT_ID} (read-only, no signing)..."
log "RPC URL: ${RPC_URL}"

RESULT="$(invoke "${MARKET_CONTRACT_ID}" "${SMOKE_FN}")"

log "Smoke invoke succeeded. ${SMOKE_FN} returned: ${RESULT:-<void>}"
echo "${RESULT}"

# ---------------------------------------------------------------------------
# Part 2 (#698): cross-contract wiring checks.
#
# Each check invokes a read-only getter that reports which counterpart
# address a contract currently has registered, then greps the raw response
# for the counterpart's own contract ID. Substring matching (rather than
# full JSON parsing) keeps this working with or without `jq`/`node` on PATH,
# same as the registry reader above — contract IDs are unique C... strings,
# so a substring match is unambiguous.
# ---------------------------------------------------------------------------
log ""
log "Wiring checks (#698): verifying each contract's counterpart addresses..."

check_wiring() {
  local description="$1" contract_id="$2" fn="$3" expect="$4"
  shift 4

  local output
  if ! output="$(invoke "${contract_id}" "${fn}" "$@" 2>&1)"; then
    log "WARN: ${description}: invoking '${fn}' on ${contract_id} failed — ${output}"
    WIRING_FAILED=1
    return
  fi

  if [[ "${output}" == *"${expect}"* ]]; then
    log "OK: ${description}"
  else
    log "FAIL: ${description}: expected '${fn}' to reference ${expect}, got: ${output}"
    WIRING_FAILED=1
  fi
}

# Market -> Outcome Token
if [[ -n "${OUTCOME_TOKEN_CONTRACT_ID}" ]]; then
  check_wiring \
    "market.get_outcome_token_contract references the configured outcome-token contract" \
    "${MARKET_CONTRACT_ID}" get_outcome_token_contract "${OUTCOME_TOKEN_CONTRACT_ID}"
else
  log "Skipping market -> outcome-token wiring check: OUTCOME_TOKEN_CONTRACT_ID not configured."
fi

# Market -> Resolution
if [[ -n "${RESOLUTION_CONTRACT_ID}" ]]; then
  check_wiring \
    "market.get_resolution_contract references the configured resolution contract" \
    "${MARKET_CONTRACT_ID}" get_resolution_contract "${RESOLUTION_CONTRACT_ID}"
else
  log "Skipping market -> resolution wiring check: RESOLUTION_CONTRACT_ID not configured."
fi

# Resolution -> Market
if [[ -n "${RESOLUTION_CONTRACT_ID}" ]]; then
  check_wiring \
    "resolution.get_config references the configured market contract" \
    "${RESOLUTION_CONTRACT_ID}" get_config "${MARKET_CONTRACT_ID}"
else
  log "Skipping resolution -> market wiring check: RESOLUTION_CONTRACT_ID not configured."
fi

# Outcome Token -> Market
if [[ -n "${OUTCOME_TOKEN_CONTRACT_ID}" ]]; then
  check_wiring \
    "outcome-token.get_config references the configured market contract" \
    "${OUTCOME_TOKEN_CONTRACT_ID}" get_config "${MARKET_CONTRACT_ID}"
else
  log "Skipping outcome-token -> market wiring check: OUTCOME_TOKEN_CONTRACT_ID not configured."
fi

# Treasury -> Market (treasury authorizes the market to call collect_fee)
if [[ -n "${TREASURY_CONTRACT_ID}" ]]; then
  check_wiring \
    "treasury.is_authorized_market(market) is true for the configured market contract" \
    "${TREASURY_CONTRACT_ID}" is_authorized_market true --market "${MARKET_CONTRACT_ID}"
else
  log "Skipping treasury -> market wiring check: TREASURY_CONTRACT_ID not configured."
fi

log ""
if [[ "${WIRING_FAILED}" -ne 0 ]]; then
  log "Wiring checks reported at least one problem — see above. Not failing the"
  log "overall smoke test on this (informational for now), but investigate before"
  log "relying on cross-contract calls in this environment."
fi

log "testnet-smoke: done."
