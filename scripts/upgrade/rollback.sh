#!/usr/bin/env bash
#
# rollback.sh — re-point the testnet contract registry back to a previous
# deployment recorded in git history (issue #664).
#
# Vatix contracts are deployed fresh on every storage-version bump — see
# contracts/market/STORAGE_MIGRATION_GUIDE.md ("Fresh deployment required.
# No data migration available."). There is no in-place WASM upgrade path, so
# "rollback" means: stop routing traffic (frontend/services/scripts) at the
# new contract ID and point them back at the previously deployed one, which
# is still live on-chain and still serves its own storage version.
#
# This script does not deploy, delete, or invoke anything on-chain — it only
# recovers the previous deployments/testnet.json registry contents from git
# history so you can review and re-commit them.
#
# Usage:
#   bash scripts/upgrade/rollback.sh [git-ref]              # dry run (default ref: HEAD~1)
#   bash scripts/upgrade/rollback.sh [git-ref] --apply       # write the ref's version over the working file
#
# IMPORTANT — this does not undo data loss:
#   Any state written ONLY to the new deployment (deposits, trades, positions
#   made after the upgrade) is not present on the old deployment. Re-pointing
#   the registry does not move that data. Read "Rollback and Recovery" in
#   contracts/market/STORAGE_MIGRATION_GUIDE.md and the "Rollback" section of
#   scripts/upgrade/UPGRADE_PLAYBOOK.md before treating this as a complete
#   recovery on a deployment that has already taken live traffic.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
REGISTRY_FILE="deployments/testnet.json"

REF="${1:-HEAD~1}"
APPLY=0
if [[ "${2:-}" == "--apply" ]]; then
  APPLY=1
fi

log() { printf '[rollback] %s\n' "$*" >&2; }

cd "${ROOT_DIR}"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  log "ERROR: not inside a git repository."
  exit 1
fi

if ! git rev-parse --verify --quiet "${REF}" >/dev/null; then
  log "ERROR: '${REF}' is not a valid git ref in this repository."
  exit 1
fi

if ! git cat-file -e "${REF}:${REGISTRY_FILE}" 2>/dev/null; then
  log "ERROR: ${REGISTRY_FILE} does not exist at ${REF}."
  exit 1
fi

log "Comparing current ${REGISTRY_FILE} against ${REF}:${REGISTRY_FILE}..."

if git diff --quiet "${REF}" -- "${REGISTRY_FILE}" 2>/dev/null; then
  log "No difference — current ${REGISTRY_FILE} already matches ${REF}. Nothing to roll back."
  exit 0
fi

log ""
log "Registry diff (${REF} -> working tree):"
git diff "${REF}" -- "${REGISTRY_FILE}" || true
log ""

if [[ "${APPLY}" -eq 0 ]]; then
  log "Dry run only — no files were changed."
  log "Re-run as 'bash scripts/upgrade/rollback.sh ${REF} --apply' to write"
  log "${REF}:${REGISTRY_FILE} over the working copy of ${REGISTRY_FILE}."
  log "Review the diff above first, then commit the change yourself."
  exit 0
fi

git show "${REF}:${REGISTRY_FILE}" >"${REGISTRY_FILE}"
log "Wrote ${REF}:${REGISTRY_FILE} to ${REGISTRY_FILE}."
log "Review with 'git diff -- ${REGISTRY_FILE}' and commit when ready."
log "Reminder: this only re-points which contract ID downstream tooling uses —"
log "it does not touch on-chain state or move data. See the data-loss warning"
log "at the top of this script before treating this as a completed rollback."
