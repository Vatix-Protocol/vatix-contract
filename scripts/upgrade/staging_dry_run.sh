#!/usr/bin/env bash
#
# staging_dry_run.sh — automated staging dry-run checklist runner.
#
# Executes the checks described in scripts/upgrade/STAGING_DRY_RUN_CHECKLIST.md
# against a staging (testnet) deployment and exits non-zero (fail-closed) if any
# check fails or is missing. Intended to be run in CI and by release engineers
# before promoting a build.
#
# Security posture:
#   * Deny-by-default: refuses to run against mainnet unless ALLOW_MAINNET=1 is
#     explicitly set AND the readiness checklist marker is present.
#   * Never prints secret values (env vars, keys, RPC URLs are redacted).
#   * Idempotent: a run lock prevents concurrent/replayed invocations.
#   * Dependency outages (RPC/DB) fail closed on any state-affecting step.
#
# Usage:
#   scripts/upgrade/staging_dry_run.sh [--network testnet|mainnet] [--run-id ID]
#
# Exit codes (stable):
#   0  all checks passed
#   2  usage / configuration error
#   3  authz denied (wrong role, missing admin, mainnet not allowed)
#   4  concurrent run detected (lock held)
#   5  dependency outage (RPC/DB unreachable)
#   6  one or more checklist checks failed
#
set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly CHECKLIST="${SCRIPT_DIR}/STAGING_DRY_RUN_CHECKLIST.md"
readonly LOCK_DIR="${TMPDIR:-/tmp}/vatix-staging-dry-run.lock"

# --- stable error codes -----------------------------------------------------
readonly E_USAGE=2
readonly E_AUTHZ=3
readonly E_CONCURRENT=4
readonly E_DEPENDENCY=5
readonly E_CHECK_FAILED=6

# --- correlation id ---------------------------------------------------------
RUN_ID="${VATIX_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
NETWORK="testnet"

log()  { printf '[staging-dry-run][%s] %s\n' "$RUN_ID" "$*"; }
warn() { printf '[staging-dry-run][%s] WARN: %s\n' "$RUN_ID" "$*" >&2; }
fail() { local code="$1"; shift; printf '[staging-dry-run][%s] ERROR(%s): %s\n' "$RUN_ID" "$code" "$*" >&2; exit "$code"; }

# Redact anything that looks like a secret before logging.
redact() {
  sed -E \
    -e 's#(https?://)[^/@[:space:]]+:[^/@[:space:]]+@#\1***:***@#g' \
    -e 's#(S[A-Z2-7]{55})#***REDACTED***#g' \
    -e 's#((SECRET|PRIVATE|ADMIN|TOKEN|KEY|PASSWORD)[A-Z_]*=)[^[:space:]]+#\1***REDACTED***#g'
}

# --- argument parsing -------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --network) NETWORK="${2:-}"; shift 2 ;;
    --run-id)  RUN_ID="${2:-}"; shift 2 ;;
    -h|--help)
      grep -E '^#( |$)' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) fail "$E_USAGE" "unknown argument: $1" ;;
  esac
done

case "$NETWORK" in
  testnet|mainnet) ;;
  *) fail "$E_USAGE" "invalid --network '$NETWORK' (expected testnet|mainnet)" ;;
esac

# --- authz: deny-by-default for mainnet -------------------------------------
if [[ "$NETWORK" == "mainnet" ]]; then
  if [[ "${ALLOW_MAINNET:-0}" != "1" ]]; then
    fail "$E_AUTHZ" "mainnet dry-run denied: set ALLOW_MAINNET=1 only after readiness checklist is signed off"
  fi
  if [[ ! -f "${SCRIPT_DIR}/MAINNET_READINESS_APPROVED" ]]; then
    fail "$E_AUTHZ" "mainnet dry-run denied: readiness marker MAINNET_READINESS_APPROVED missing"
  fi
  warn "running against MAINNET with explicit approval"
fi

# --- authz: require an admin/operator role ----------------------------------
if [[ -z "${VATIX_ADMIN_ROLE:-}" ]]; then
  fail "$E_AUTHZ" "missing VATIX_ADMIN_ROLE; deny-by-default for privileged dry-run"
fi
case "${VATIX_ADMIN_ROLE}" in
  admin|operator|release-manager) ;;
  *) fail "$E_AUTHZ" "role '${VATIX_ADMIN_ROLE}' is not authorized for staging dry-run" ;;
esac

# --- idempotency: single-flight lock ----------------------------------------
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
  fail "$E_CONCURRENT" "another dry-run is in progress (lock: $LOCK_DIR); replayed/concurrent runs are rejected"
fi
trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT

# --- checklist presence -----------------------------------------------------
if [[ ! -f "$CHECKLIST" ]]; then
  fail "$E_CHECK_FAILED" "checklist not found: $CHECKLIST"
fi

# --- dependency probe (fail closed on writes) -------------------------------
probe_dependency() {
  local name="$1" url="$2"
  if [[ -z "$url" ]]; then
    warn "$name endpoint not configured; treating as outage (fail-closed)"
    return 1
  fi
  if ! curl -fsS --max-time 10 "$url" >/dev/null 2>&1; then
    warn "$name unreachable at $(printf '%s' "$url" | redact)"
    return 1
  fi
  return 0
}

if ! probe_dependency "RPC" "${VATIX_RPC_HEALTH_URL:-}"; then
  fail "$E_DEPENDENCY" "RPC dependency outage; refusing to run state-affecting steps"
fi

# --- run checklist steps ----------------------------------------------------
# Each step is a shell command derived from the checklist. A non-zero exit or a
# missing command fails the whole run (fail-closed).
FAILED=0
run_step() {
  local id="$1" desc="$2"; shift 2
  log "step ${id}: ${desc}"
  if "$@" >/dev/null 2>&1; then
    log "step ${id}: PASS"
  else
    warn "step ${id}: FAIL (${desc})"
    FAILED=1
  fi
}

# 1. Contract builds and unit tests pass.
run_step "build" "cargo build --workspace" cargo build --workspace
run_step "unit"  "cargo test --workspace"  cargo test --workspace

# 2. Market invariants hold on staging snapshot.
run_step "invariants" "market invariant check" \
  cargo run --quiet --bin market-invariants -- --network "$NETWORK"

# 3. Address drift check: deployed addresses match expected manifest.
run_step "address-drift" "verify deployed addresses" \
  cargo run --quiet --bin verify-addresses -- --network "$NETWORK"

if [[ "$FAILED" -ne 0 ]]; then
  fail "$E_CHECK_FAILED" "one or more staging dry-run checks failed; see log above"
fi

log "all staging dry-run checks passed (network=${NETWORK})"
exit 0
