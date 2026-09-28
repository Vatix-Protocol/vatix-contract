#!/usr/bin/env bash
# check-auth-table.sh
#
# #892 — PR template requires AUTH_TABLE updates.
#
# Enforcement half of the "Authorization (AUTH_TABLE)" section in
# .github/PULL_REQUEST_TEMPLATE.md: adding, renaming, removing, or re-gating a
# privileged entrypoint without updating AUTH_TABLE.md turns CI red instead of
# relying on reviewers to notice. Two checks run for every contract:
#
#   1. Coverage — every `pub fn` inside the `#[contractimpl]` block of
#      contracts/<crate>/src/lib.rs whose body calls `require_auth(` must
#      appear in backticks somewhere in AUTH_TABLE.md (as a table row, or in
#      the user-facing paragraph under that contract's table).
#   2. Staleness — every backticked name in the first column of a table row
#      under that contract's "## <Name> contract" heading must still be a
#      `pub fn` of that contract, so removed/renamed entrypoints cannot linger.
#
# Fails closed: a missing lib.rs / AUTH_TABLE.md / contract heading, or a
# crate in which zero auth-gated entrypoints are parsed (parser drift), is an
# error, not a pass.
#
# Exit codes:
#   0  — the table covers every auth-gated entrypoint and has no stale rows
#   1  — an entrypoint is undocumented, a row is stale, or the check could not run
#
# Usage:
#   bash scripts/check-auth-table.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TABLE="$ROOT/AUTH_TABLE.md"

# crate directory -> AUTH_TABLE.md section heading (matched as a line prefix,
# e.g. "## Market contract (`contracts/market/src/lib.rs`)")
declare -A HEADINGS=(
  [market]="## Market contract"
  [treasury]="## Treasury contract"
  [resolution]="## Resolution contract"
  [outcome-token]="## Outcome-token contract"
)
CRATES=(market treasury resolution outcome-token)

if [[ ! -f "$TABLE" ]]; then
  echo "ERROR: $TABLE not found." >&2
  exit 1
fi

# Print every entrypoint (`pub fn`) in the #[contractimpl] block of $1, one per
# line, as "<name> <1 if its body calls require_auth( else 0>". The block ends
# at the first column-0 `}` after `#[contractimpl]`; a non-pub `fn` inside it
# closes the previous entrypoint's body so helper code is never attributed to
# an entrypoint. Comment lines are ignored.
entrypoints() {
  awk '
    function flush() { if (name != "") print name, auth; name = "" }
    /^#\[contractimpl\]/ { in_impl = 1; next }
    !in_impl { next }
    /^}/ { flush(); exit }
    /^[[:space:]]*pub fn [A-Za-z_0-9]+/ {
      flush()
      match($0, /pub fn [A-Za-z_0-9]+/)
      name = substr($0, RSTART + 7, RLENGTH - 7); auth = 0; next
    }
    /^[[:space:]]*(pub\([a-z]+\) )?fn [A-Za-z_0-9]+/ { flush(); next }
    /^[[:space:]]*\/\// { next }
    /require_auth\(/ { auth = 1 }
  ' "$1"
}

# Print the backticked names in the first column of every table row in the
# AUTH_TABLE.md section whose heading starts with $1 (up to the next "## ").
table_rows() {
  awk -v heading="$1" '
    index($0, heading) == 1 { in_sec = 1; next }
    in_sec && /^## / { exit }
    in_sec && /^\| `/ {
      split($0, cells, "|"); first = cells[2]
      while (match(first, /`[A-Za-z_0-9]+`/)) {
        print substr(first, RSTART + 1, RLENGTH - 2)
        first = substr(first, RSTART + RLENGTH)
      }
    }
  ' "$TABLE" | sort -u
}

FAILED=0
TOTAL=0

for crate in "${CRATES[@]}"; do
  lib="$ROOT/contracts/$crate/src/lib.rs"
  heading="${HEADINGS[$crate]}"
  if [[ ! -f "$lib" ]]; then
    echo "ERROR: $lib not found." >&2
    FAILED=1
    continue
  fi
  if ! grep -q "^${heading}" "$TABLE"; then
    echo "ERROR: AUTH_TABLE.md has no \"$heading\" section." >&2
    FAILED=1
    continue
  fi

  all=$(entrypoints "$lib")
  gated=$(awk '$2 == 1 { print $1 }' <<< "$all" | sort -u)
  if [[ -z "$gated" ]]; then
    echo "ERROR: no auth-gated entrypoints parsed from $lib — the parser in" >&2
    echo "       scripts/check-auth-table.sh is out of date with the source layout." >&2
    FAILED=1
    continue
  fi

  # 1. Coverage
  while IFS= read -r fn_name; do
    TOTAL=$((TOTAL + 1))
    if ! grep -qF "\`$fn_name\`" "$TABLE"; then
      echo "ERROR: $crate::$fn_name calls require_auth() but is not documented in AUTH_TABLE.md." >&2
      FAILED=1
    fi
  done <<< "$gated"

  # 2. Staleness
  while IFS= read -r row_name; do
    [[ -z "$row_name" ]] && continue
    if ! awk '{ print $1 }' <<< "$all" | grep -qxF "$row_name"; then
      echo "ERROR: AUTH_TABLE.md \"$heading\" lists \`$row_name\`, which is not an entrypoint of contracts/$crate." >&2
      FAILED=1
    fi
  done <<< "$(table_rows "$heading")"
done

if [[ $FAILED -ne 0 ]]; then
  echo "" >&2
  echo "Update AUTH_TABLE.md in the same PR (see the \"Authorization (AUTH_TABLE)\"" >&2
  echo "section of .github/PULL_REQUEST_TEMPLATE.md), then re-run:" >&2
  echo "  bash scripts/check-auth-table.sh" >&2
  exit 1
fi

echo "AUTH_TABLE check passed: $TOTAL auth-gated entrypoints documented, no stale rows, across ${#CRATES[@]} contracts."
