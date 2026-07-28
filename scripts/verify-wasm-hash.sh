#!/usr/bin/env bash
#
# verify-wasm-hash.sh - Verify WASM artifact consistency
#
# This script builds the contract and displays the SHA256 hash of the
# resulting WASM file. When an expected hash is supplied, the script verifies
# that the built artifact matches it and exits non-zero on mismatch.
#
# Usage:
#   bash scripts/verify-wasm-hash.sh [contract-dir] [expected-sha256]
#
# Examples:
#   bash scripts/verify-wasm-hash.sh                              # defaults to contracts/market
#   bash scripts/verify-wasm-hash.sh contracts/treasury
#   bash scripts/verify-wasm-hash.sh contracts/market "$WASM_HASH"
#   EXPECTED_WASM_HASH="$WASM_HASH" bash scripts/verify-wasm-hash.sh contracts/market
#
set -euo pipefail

CONTRACT_DIR="${1:-contracts/market}"
EXPECTED_HASH="${2:-${EXPECTED_WASM_HASH:-}}"

log() { printf '[verify-wasm-hash] %s\n' "$*" >&2; }

if ! command -v stellar >/dev/null 2>&1; then
  log "ERROR: 'stellar' CLI not found on PATH."
  log "Install it from https://developers.stellar.org/docs/tools/cli"
  exit 127
fi

if [[ ! -d "${CONTRACT_DIR}" ]]; then
  log "ERROR: Contract directory not found: ${CONTRACT_DIR}"
  exit 1
fi

log "Building ${CONTRACT_DIR} using stellar contract build..."
stellar contract build --manifest-path "${CONTRACT_DIR}/Cargo.toml"

# Find the WASM artifact
WASM_PATH="$(find target/wasm32v1-none/release -maxdepth 1 -name '*.wasm' 2>/dev/null | grep -v '.opt.wasm' | head -n1 || true)"

if [[ -z "${WASM_PATH}" || ! -f "${WASM_PATH}" ]]; then
  log "ERROR: Could not locate WASM artifact in target/wasm32v1-none/release/"
  exit 1
fi

log "WASM artifact: ${WASM_PATH}"
log "File size: $(du -h "${WASM_PATH}" | cut -f1)"
log ""
log "SHA256 hash:"

# Compute and display hash (works on both Linux and macOS)
if command -v sha256sum >/dev/null 2>&1; then
  HASH_LINE="$(sha256sum "${WASM_PATH}")"
elif command -v shasum >/dev/null 2>&1; then
  HASH_LINE="$(shasum -a 256 "${WASM_PATH}")"
else
  log "ERROR: Neither sha256sum nor shasum found on PATH"
  exit 1
fi

printf '%s\n' "${HASH_LINE}"

ACTUAL_HASH="$(printf '%s\n' "${HASH_LINE}" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')"

if [[ -n "${EXPECTED_HASH}" ]]; then
  NORMALIZED_EXPECTED="$(printf '%s' "${EXPECTED_HASH}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${ACTUAL_HASH}" != "${NORMALIZED_EXPECTED}" ]]; then
    log "ERROR: WASM hash mismatch."
    log "Expected: ${NORMALIZED_EXPECTED}"
    log "Actual:   ${ACTUAL_HASH}"
    exit 1
  fi

  log "Hash matches expected value."
else
  log "No expected hash provided; skipping match verification."
fi

log ""
log "✓ To verify consistency, compare this hash with builds from other environments"
log "  (e.g., CI artifacts, other developer machines)."
