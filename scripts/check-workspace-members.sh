#!/usr/bin/env bash
# check-workspace-members.sh
#
# #779 — Workspace members guard.
#
# Ensures only the four production crates (market, treasury, resolution,
# outcome-token) are present under contracts/ and that no stray crates
# (hello-world, scratch, examples, etc.) have been added to the workspace.
#
# Exit codes:
#   0  — workspace is clean; only the allowed crates are present
#   1  — unexpected crate(s) found; prints each offending path and fails CI
#
# Usage:
#   bash scripts/check-workspace-members.sh
#
# The script discovers every Cargo.toml under contracts/ and verifies that
# each parent directory is in the allowlist below.  Add a new production
# crate to ALLOWED_CRATES when the workspace genuinely grows.

set -euo pipefail

CONTRACTS_DIR="$(cd "$(dirname "$0")/.." && pwd)/contracts"

# Canonical set of production crates under contracts/.
ALLOWED_CRATES=(
  "market"
  "treasury"
  "resolution"
  "outcome-token"
)

# Build a quick lookup from the allowlist.
declare -A ALLOWED_MAP
for name in "${ALLOWED_CRATES[@]}"; do
  ALLOWED_MAP["$name"]=1
done

FAILED=0
FOUND=()

# Walk every direct sub-directory of contracts/ that contains a Cargo.toml.
while IFS= read -r cargo_toml; do
  crate_dir=$(dirname "$cargo_toml")
  crate_name=$(basename "$crate_dir")

  FOUND+=("$crate_name")

  if [[ -z "${ALLOWED_MAP[$crate_name]+_}" ]]; then
    echo "ERROR: Unexpected crate '${crate_name}' found at '${crate_dir}'." >&2
    echo "       Only the following crates are allowed under contracts/:" >&2
    for a in "${ALLOWED_CRATES[@]}"; do
      echo "         - $a" >&2
    done
    echo "       Remove the crate or add it to ALLOWED_CRATES in this script" >&2
    echo "       once it has been reviewed and approved for production." >&2
    FAILED=1
  fi
done < <(find "$CONTRACTS_DIR" -maxdepth 2 -name "Cargo.toml" | sort)

if [[ ${#FOUND[@]} -eq 0 ]]; then
  echo "ERROR: No Cargo.toml files found under contracts/. Is the path correct?" >&2
  exit 1
fi

if [[ $FAILED -ne 0 ]]; then
  exit 1
fi

echo "workspace-members check passed: $(IFS=', '; echo "${FOUND[*]}")"
