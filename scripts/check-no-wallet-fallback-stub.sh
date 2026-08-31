#!/usr/bin/env bash
# Fail closed without GSTUB (Issue #700).
#
# The Freighter wallet integration must never silently fall back to a stub
# address when the extension is missing/locked/rejected — that fallback
# masks real misconfiguration (a user thinks they're connected when they're
# not, and every subsequent contract call fails with a confusing on-chain
# error instead of a clear "connect your wallet" message). This script is
# the automated regression guard the issue calls for: it fails CI outright
# if that pattern — or the specific `GSTUB` stub address it used — is ever
# reintroduced anywhere under apps/web.
#
# Usage: bash scripts/check-no-wallet-fallback-stub.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB_DIR="${ROOT_DIR}/apps/web"

fail=0

# 1. The specific historical stub address must never reappear anywhere.
if grep -rn "GSTUB" "${WEB_DIR}" --include="*.ts" --include="*.tsx" 2>/dev/null; then
  echo ""
  echo "ERROR: Found a 'GSTUB' wallet stub address under apps/web."
  echo "Freighter integration must fail closed (surface a connect error)"
  echo "instead of silently falling back to a placeholder address."
  echo "See docs/freighter-integration-guide.md."
  fail=1
fi

# 2. Generically: setAddress()/setState() must never be called with a
#    string literal in the wallet connection path — a real address only
#    ever comes from Freighter's getAddress() response. A literal there is
#    a fallback stub by construction.
if grep -rnE 'setAddress\(\s*"' "${WEB_DIR}/context" 2>/dev/null; then
  echo ""
  echo "ERROR: setAddress() called with a string literal in apps/web/context."
  echo "A wallet address must only ever come from Freighter's getAddress()"
  echo "response — never a hardcoded fallback. Surface a connect error instead."
  fail=1
fi

if [[ "${fail}" -eq 0 ]]; then
  echo "OK: no wallet fallback stub found."
fi

exit "${fail}"
