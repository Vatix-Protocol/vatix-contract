# Issue #968: Hardening follow-up 6 - residual authz/observability/docs gaps

## Overview

This issue closes the remaining residual gap in the hardening sequence for authz, traceability, and the documentation model used by contributors and operators. The work is intentionally about production-grade safety controls rather than a copyedit or cosmetic improvement.

## Reference set

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/freighter-integration-guide.md`](docs/freighter-integration-guide.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`scripts/issues/README.md`](scripts/issues/README.md)

## Invariants

1. The contract remains the authority for balances, settlement, and admin actions.
2. Untrusted clients do not bypass the on-chain authz gate or network checks.
3. Every money-path write remains traceable through stable error codes and correlation ids.
4. Logs and metrics remain safe for production operations and never include raw secrets or signed payloads.
5. Every mainnet-affecting path remains behind an emergency stop or documented rollback path.

## Completion summary

- The authz and fail-closed posture is explicitly preserved across security and contributor-facing docs.
- The client and operator guidance calls out that wallet-connected clients are untrusted and cannot determine protocol state on their own.
- Runbook and issue-script guidance reinforce idempotency, dependency-outage fail-closed behavior, and safe observability.
- The repo documents the operating expectation that money-path actions are gated and rollback-ready.

## Acceptance criteria record

- [x] authz and network checks documented
- [x] correlation-id and safe-logging guidance captured
- [x] wallet/client trust assumptions clarified
- [x] runbook/ops links preserved
- [x] kill-switch and rollback requirements included

## Status

Archived as complete within the hardening sequence.
