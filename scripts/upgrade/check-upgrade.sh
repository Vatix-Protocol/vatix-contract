#!/usr/bin/env bash
#
# check-upgrade.sh — cross-contract upgrade dry-run (issue #664).
#
# One scripted entry point an operator (or CI) can run to get a pass/fail
# verdict before rolling an upgrade out to the four Vatix contracts (market,
# treasury, resolution, outcome-token). It runs three phases:
#
#   Phase A — Storage-version drift check (always runs, no external tools
#             needed beyond jq). Compares the STORAGE_VERSION constant
#             compiled into contracts/{market,treasury,resolution,outcome-token}/src/storage.rs
#             against the recorded value in version-matrix.json and fails on
#             any mismatch. As of issue #696 all four contracts carry a
#             STORAGE_VERSION constant; check_version_drift also still
#             handles the (now hypothetical) case of an unversioned contract
#             whose absence isn't correctly reflected in the matrix.
#
#   Phase B — WASM hash verification (guarded on the `stellar` CLI being on
#             PATH). Builds each contract via `stellar contract build` and
#             compares its SHA-256 hash against expected-hashes.json. An
#             unpinned (empty) expected hash is a warning, not a failure; a
#             pinned hash that doesn't match the freshly built artifact is a
#             failure.
#
#   Phase C — UpgradeRequired regression tests (guarded on `cargo` being on
#             PATH). Runs the existing version-guard unit tests for all four
#             versioned contracts (market, treasury, resolution,
#             outcome-token — see #696) to simulate the upgrade-required
#             code path before it's ever hit in production.
#
# See scripts/upgrade/UPGRADE_PLAYBOOK.md for the full playbook this script
# is one step of.
#
# Usage:
#   bash scripts/upgrade/check-upgrade.sh
#
# Exit code: 0 = pass (including phases skipped in guard mode), 1 = fail.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MATRIX_FILE="${SCRIPT_DIR}/version-matrix.json"
HASHES_FILE="${SCRIPT_DIR}/expected-hashes.json"

log() { printf '[check-upgrade] %s\n' "$*" >&2; }

FAILED=0
fail() {
  log "FAIL: $*"
  FAILED=1
}
warn() { log "WARN: $*"; }
ok() { log "OK: $*"; }

for f in "${MATRIX_FILE}" "${HASHES_FILE}"; do
  if [[ ! -f "${f}" ]]; then
    log "ERROR: required manifest not found: ${f}"
    exit 1
  fi
done

if ! command -v jq >/dev/null 2>&1; then
  log "ERROR: 'jq' is required to parse version-matrix.json / expected-hashes.json."
  exit 127
fi

if ! jq empty "${MATRIX_FILE}" 2>/dev/null; then
  log "ERROR: ${MATRIX_FILE} is not valid JSON."
  exit 1
fi
if ! jq empty "${HASHES_FILE}" 2>/dev/null; then
  log "ERROR: ${HASHES_FILE} is not valid JSON."
  exit 1
fi

# ── Phase A: storage-version drift ──────────────────────────────────────────
log "Phase A: storage-version drift check"

check_version_drift() {
  local name="$1" storage_rs="$2"
  local source_version matrix_version

  if [[ ! -f "${storage_rs}" ]]; then
    fail "${name}: storage module not found at ${storage_rs}"
    return
  fi

  source_version="$(grep -oE 'pub const STORAGE_VERSION: u32 = [0-9]+' "${storage_rs}" 2>/dev/null | grep -oE '[0-9]+$' || true)"
  matrix_version="$(jq -r ".contracts.${name}.storageVersion // empty" "${MATRIX_FILE}")"

  if [[ -z "${source_version}" ]]; then
    if [[ -n "${matrix_version}" ]]; then
      fail "${name}: version-matrix.json declares storageVersion=${matrix_version} but no STORAGE_VERSION constant was found in ${storage_rs}"
    else
      warn "${name}: unversioned contract (no STORAGE_VERSION constant) — version-matrix.json correctly marks it wasmHashOnly"
    fi
    return
  fi

  if [[ -z "${matrix_version}" ]]; then
    fail "${name}: STORAGE_VERSION=${source_version} in source but version-matrix.json has no storageVersion recorded — update scripts/upgrade/version-matrix.json in the same PR that bumped it"
    return
  fi

  if [[ "${source_version}" != "${matrix_version}" ]]; then
    fail "${name}: STORAGE_VERSION drift — source=${source_version}, version-matrix.json=${matrix_version}. Update scripts/upgrade/version-matrix.json to match."
    return
  fi

  ok "${name}: storage version ${source_version} matches version-matrix.json"
}

check_version_drift "market" "${ROOT_DIR}/contracts/market/src/storage.rs"
check_version_drift "treasury" "${ROOT_DIR}/contracts/treasury/src/storage.rs"
check_version_drift "resolution" "${ROOT_DIR}/contracts/resolution/src/storage.rs"
check_version_drift "outcomeToken" "${ROOT_DIR}/contracts/outcome-token/src/storage.rs"

# ── Phase B: WASM hash verification ─────────────────────────────────────────
log ""
log "Phase B: WASM hash verification"

if ! command -v stellar >/dev/null 2>&1; then
  warn "'stellar' CLI not found on PATH — skipping build + hash verification (guard mode)."
  warn "Install it from https://developers.stellar.org/docs/tools/cli to run the full dry-run."
else
  check_hash() {
    local name="$1"
    local manifest wasm_file expected actual wasm_path

    manifest="$(jq -r ".contracts.${name}.manifestPath" "${HASHES_FILE}")"
    wasm_file="$(jq -r ".contracts.${name}.wasmFile" "${HASHES_FILE}")"
    expected="$(jq -r ".contracts.${name}.expectedSha256 // empty" "${HASHES_FILE}")"

    log "Building ${manifest}..."
    if ! stellar contract build --manifest-path "${ROOT_DIR}/${manifest}" >/dev/null; then
      fail "${name}: 'stellar contract build' failed for ${manifest}"
      return
    fi

    wasm_path="${ROOT_DIR}/target/wasm32v1-none/release/${wasm_file}"
    if [[ ! -f "${wasm_path}" ]]; then
      fail "${name}: expected WASM artifact not found at ${wasm_path}"
      return
    fi

    if command -v sha256sum >/dev/null 2>&1; then
      actual="$(sha256sum "${wasm_path}" | awk '{print $1}')"
    else
      actual="$(shasum -a 256 "${wasm_path}" | awk '{print $1}')"
    fi

    if [[ -z "${expected}" ]]; then
      if [[ "${ALLOW_UNPINNED_HASHES:-}" == "1" ]]; then
        warn "${name}: no expectedSha256 pinned in expected-hashes.json yet (built hash: ${actual}) — ALLOW_UNPINNED_HASHES=1 set, not failing"
      else
        fail "${name}: no expectedSha256 pinned in expected-hashes.json (built hash: ${actual}). Fail-closed by default for audit/mainnet readiness (Issue #697) — pin it via scripts/verify-wasm-hash.sh, or set ALLOW_UNPINNED_HASHES=1 for local/dev dry-runs."
      fi
      return
    fi

    if [[ "${expected}" != "${actual}" ]]; then
      fail "${name}: WASM hash mismatch — expected=${expected} actual=${actual}"
      return
    fi

    ok "${name}: WASM hash matches pinned value (${actual})"
  }

  check_hash "market"
  check_hash "treasury"
  check_hash "resolution"
  check_hash "outcomeToken"
fi

# ── Phase C: simulate UpgradeRequired paths ─────────────────────────────────
log ""
log "Phase C: UpgradeRequired regression tests"

if ! command -v cargo >/dev/null 2>&1; then
  warn "'cargo' not found on PATH — skipping UpgradeRequired regression tests (guard mode)."
else
  run_version_tests() {
    local crate="$1"
    log "Running version-guard tests for ${crate}..."
    if ! (cd "${ROOT_DIR}" && cargo test -p "${crate}" version >/dev/null); then
      fail "${crate}: version-guard tests failed"
      return
    fi
    ok "${crate}: version-guard tests passed"
  }

  run_version_tests "vatix-market-contract"
  run_version_tests "vatix-treasury-contract"
  run_version_tests "vatix-resolution-contract"
  run_version_tests "vatix-outcome-token-contract"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
log ""
if [[ "${FAILED}" -ne 0 ]]; then
  log "RESULT: FAIL — see above for details."
  exit 1
fi
log "RESULT: PASS"
exit 0
