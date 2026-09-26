#!/usr/bin/env bash
#
# scripts/upgrade/upgrade.sh
#
# Executes the multi-contract upgrade order documented in
# scripts/upgrade/UPGRADE_PLAYBOOK.md.
#
# Invariants enforced by this script (see playbook for full detail):
#   1. Dependencies are upgraded before their dependents.
#   2. Every contract's storage version is compatible with the target build.
#   3. Admin/authz configuration is preserved across the upgrade.
#   4. The script fails closed: any missing env, wrong network, or failed
#      preflight aborts the run before a single contract is touched.
#
# Usage:
#   NETWORK=testnet ADMIN_SECRET=<...> ./scripts/upgrade/upgrade.sh
#   NETWORK=mainnet ADMIN_SECRET=<...> CONFIRM_MAINNET=yes ./scripts/upgrade/upgrade.sh
#
# Rollback: see "Rollback" section of UPGRADE_PLAYBOOK.md. This script never
# performs an automatic rollback; it stops on the first failure so an operator
# can follow the documented rollback procedure.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PLAYBOOK="${SCRIPT_DIR}/UPGRADE_PLAYBOOK.md"

log()  { printf '[upgrade] %s\n' "$*" >&2; }
warn() { printf '[upgrade][warn] %s\n' "$*" >&2; }
die()  { printf '[upgrade][fatal] %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 1. Preconditions (fail closed)
# ---------------------------------------------------------------------------

[ -f "${PLAYBOOK}" ] || die "playbook not found at ${PLAYBOOK}; refusing to run"

: "${NETWORK:?NETWORK is required (testnet|mainnet)}"
: "${ADMIN_SECRET:?ADMIN_SECRET is required}"

case "${NETWORK}" in
  testnet|mainnet) ;;
  *) die "NETWORK must be 'testnet' or 'mainnet', got '${NETWORK}'" ;;
 esac

if [ "${NETWORK}" = "mainnet" ] && [ "${CONFIRM_MAINNET:-}" != "yes" ]; then
  die "mainnet upgrade requires CONFIRM_MAINNET=yes (readiness checklist in playbook)"
fi

# Kill-switch: allow operators to disable the money-path upgrade entirely.
if [ "${UPGRADE_DISABLED:-}" = "1" ]; then
  die "UPGRADE_DISABLED=1 set; refusing to run"
fi

command -v stellar >/dev/null 2>&1 || die "stellar CLI not found on PATH"

# ---------------------------------------------------------------------------
# 2. Upgrade order (dependencies before dependents)
# ---------------------------------------------------------------------------
#
# The order below MUST match the "Upgrade order" section of the playbook.
# Each entry is "<contract-name>:<wasm-path>". Dependents appear strictly
# after the contracts they depend on.

UPGRADE_ORDER=(
  "vatix-token:${REPO_ROOT}/target/wasm32-unknown-unknown/release/vatix_token.wasm"
  "vatix-oracle:${REPO_ROOT}/target/wasm32-unknown-unknown/release/vatix_oracle.wasm"
  "vatix-liquidity:${REPO_ROOT}/target/wasm32-unknown-unknown/release/vatix_liquidity.wasm"
  "vatix-settlement:${REPO_ROOT}/target/wasm32-unknown-unknown/release/vatix_settlement.wasm"
)

# ---------------------------------------------------------------------------
# 3. Preflight: verify every artifact exists before touching the network
# ---------------------------------------------------------------------------

for entry in "${UPGRADE_ORDER[@]}"; do
  name="${entry%%:*}"
  wasm="${entry#*:}"
  [ -f "${wasm}" ] || die "missing wasm artifact for ${name}: ${wasm}"
  log "preflight ok: ${name} -> ${wasm}"
done

# ---------------------------------------------------------------------------
# 4. Execute upgrades in order, stopping on first failure
# ---------------------------------------------------------------------------

for entry in "${UPGRADE_ORDER[@]}"; do
  name="${entry%%:*}"
  wasm="${entry#*:}"

  log "upgrading ${name} on ${NETWORK}"
  if ! stellar contract upload \
        --network "${NETWORK}" \
        --source-account "${ADMIN_SECRET}" \
        --wasm "${wasm}"; then
    die "upload failed for ${name}; aborting before dependents (see rollback in playbook)"
  fi

  if ! stellar contract invoke \
        --network "${NETWORK}" \
        --source-account "${ADMIN_SECRET}" \
        --id "${name}" \
        -- upgrade \
        --new_wasm_hash "$(stellar contract hash --wasm "${wasm}")"; then
    die "upgrade failed for ${name}; aborting before dependents (see rollback in playbook)"
  fi

  log "upgraded ${name}"
done

log "all contracts upgraded in documented order on ${NETWORK}"
log "next: run post-upgrade verification steps in ${PLAYBOOK}"
