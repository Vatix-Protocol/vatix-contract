#!/usr/bin/env bash
#
# invoke-example.sh — smoke-test a deployed market contract by invoking a
# read-only function and asserting the call succeeds.
#
# This replaces the previous echo guard with a real `stellar contract invoke`
# call. It is run in CI immediately after deploy-testnet.sh to prove the freshly
# deployed contract is reachable and callable on-chain.
#
# Requirements:
#   - The `stellar` CLI on PATH.
#   - CONTRACT_ID            The deployed contract ID (from deploy-testnet.sh).
#   - TESTNET_SECRET_KEY     A funded testnet account secret key (S...) used as
#                            the invocation source account.
#
# Optional environment overrides:
#   SOROBAN_NETWORK   Network to invoke against (default: testnet)
#   SMOKE_FN          Read-only function to call (default: get_treasury)
#
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
SMOKE_FN="${SMOKE_FN:-get_treasury}"

log() { printf '[invoke-example] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  exit 127
fi

if [[ -z "${CONTRACT_ID:-}" ]]; then
  log "ERROR: CONTRACT_ID is not set. Run deploy-testnet.sh first and pass its"
  log "output (the contract ID) in via the CONTRACT_ID environment variable."
  exit 1
fi

if [[ -z "${TESTNET_SECRET_KEY:-}" ]]; then
  log "ERROR: TESTNET_SECRET_KEY is not set (needed as the invocation source)."
  exit 1
fi

log "Smoke-invoking '${SMOKE_FN}' on contract ${CONTRACT_ID} (network: ${NETWORK})..."

# Invoke a read-only function. A zero exit status proves the deployed contract
# is callable on-chain; the returned value (e.g. `null` for an unset treasury)
# is printed for visibility.
RESULT="$(
  stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --source-account "${TESTNET_SECRET_KEY}" \
    --network "${NETWORK}" \
    -- "${SMOKE_FN}"
)"

log "Smoke invoke succeeded. ${SMOKE_FN} returned: ${RESULT:-<void>}"
echo "${RESULT}"
