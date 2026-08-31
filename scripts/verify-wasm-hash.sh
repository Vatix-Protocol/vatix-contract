#!/usr/bin/env bash
#
# verify-wasm-hash.sh - Verify WASM artifact consistency
#
# This script builds the contract and displays the SHA256 hash of the
# resulting WASM file. Use it to verify that builds are reproducible
# across different environments (local, CI, etc.).
#
# Usage:
#   bash scripts/verify-wasm-hash.sh [contract-dir]
#
# Examples:
#   bash scripts/verify-wasm-hash.sh                    # defaults to contracts/market
#   bash scripts/verify-wasm-hash.sh contracts/treasury
#
set -euo pipefail

CONTRACT_DIR="${1:-contracts/market}"

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
  sha256sum "${WASM_PATH}"
elif command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "${WASM_PATH}"
else
  log "ERROR: Neither sha256sum nor shasum found on PATH"
  exit 1
fi

log ""
log "✓ To verify consistency, compare this hash with builds from other environments"
log "  (e.g., CI artifacts, other developer machines)."
