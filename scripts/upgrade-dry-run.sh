#!/usr/bin/env bash
# scripts/upgrade-dry-run.sh
#
# Dry-run upgrade simulation for a Vatix Soroban contract.
#
# Runs all upgrade steps with --send=no (simulate-only) so no ledger state
# is mutated. Safe to run in CI or locally against a funded testnet account.
#
# See docs/upgrade-dry-run.md for the full step-by-step guide and checklist.
#
# Required environment variables:
#   TESTNET_SECRET_KEY   — Funded testnet account secret key (S...)
#   OLD_CONTRACT_ID      — Deployed contract ID to verify version-lockout on
#
# Optional environment variables:
#   CONTRACT_DIR         — Contract directory to build (default: contracts/market)
#   SOROBAN_NETWORK      — Network name (default: testnet)
#   SOROBAN_RPC_URL      — Custom RPC URL (overrides network default)
#   NETWORK_PASSPHRASE   — Custom network passphrase

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────────

CONTRACT_DIR="${CONTRACT_DIR:-contracts/market}"
NETWORK="${SOROBAN_NETWORK:-testnet}"

if [[ -z "${TESTNET_SECRET_KEY:-}" ]]; then
  echo "ERROR: TESTNET_SECRET_KEY is not set." >&2
  echo "       Export a funded testnet account secret key before running." >&2
  exit 1
fi

# ── Step 1: Build ──────────────────────────────────────────────────────────────

echo "==> [1/4] Building contract WASM in ${CONTRACT_DIR} ..."
(cd "${CONTRACT_DIR}" && stellar contract build)

# Locate the WASM artifact
WASM_PATH="$(find target/wasm32v1-none/release -name '*.wasm' | head -1)"
if [[ -z "${WASM_PATH}" ]]; then
  echo "ERROR: No WASM artifact found under target/wasm32v1-none/release/" >&2
  exit 1
fi
echo "    WASM: ${WASM_PATH}"

# ── Step 2: Simulate upload ────────────────────────────────────────────────────

echo "==> [2/4] Simulating contract upload (--send=no) ..."
UPLOAD_OUTPUT=$(stellar contract upload \
  --wasm "${WASM_PATH}" \
  --source "${TESTNET_SECRET_KEY}" \
  --network "${NETWORK}" \
  --send=no 2>&1)
echo "${UPLOAD_OUTPUT}"

# Extract WASM hash from simulation output (preflight returns it in the result).
WASM_HASH=$(echo "${UPLOAD_OUTPUT}" | grep -oE '[0-9a-f]{64}' | head -1 || true)
if [[ -z "${WASM_HASH}" ]]; then
  echo "WARN: Could not extract WASM hash from simulation output."
  echo "      Skipping upgrade simulation step."
else
  echo "    WASM hash (simulated): ${WASM_HASH}"

  # ── Step 3: Simulate upgrade call ───────────────────────────────────────────

  if [[ -n "${OLD_CONTRACT_ID:-}" ]]; then
    echo "==> [3/4] Simulating upgrade invocation on ${OLD_CONTRACT_ID} (--send=no) ..."
    stellar contract invoke \
      --id "${OLD_CONTRACT_ID}" \
      --source "${TESTNET_SECRET_KEY}" \
      --network "${NETWORK}" \
      --send=no \
      -- upgrade \
      --new_wasm_hash "${WASM_HASH}" \
      || echo "WARN: Upgrade simulation returned a non-zero exit — check output above."
  else
    echo "==> [3/4] OLD_CONTRACT_ID not set — skipping upgrade invocation simulation."
  fi
fi

# ── Step 4: Verify storage version (read-only) ────────────────────────────────

if [[ -n "${OLD_CONTRACT_ID:-}" ]]; then
  echo "==> [4/4] Verifying old deployment storage-version gate (--send=no) ..."
  echo "    Expecting: Error(Contract, #70) — UpgradeRequired"
  stellar contract invoke \
    --id "${OLD_CONTRACT_ID}" \
    --source "${TESTNET_SECRET_KEY}" \
    --network "${NETWORK}" \
    --send=no \
    -- get_admin 2>&1 \
    | grep -q "UpgradeRequired\|#70" \
    && echo "    OK: old deployment correctly locked." \
    || echo "    WARN: old deployment did not return UpgradeRequired — verify manually."
else
  echo "==> [4/4] OLD_CONTRACT_ID not set — skipping version-lockout check."
fi

echo ""
echo "==> Dry-run complete. No transactions were submitted."
echo "    Review docs/upgrade-dry-run.md for the full deployment checklist."
