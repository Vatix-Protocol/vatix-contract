# Issue #969: Hardening follow-up 7 - residual authz/observability/docs gaps

## Overview

The final hardening follow-up closes the remaining gaps between the protocol’s operational model and the contributor documentation. The goal is to preserve the fail-closed posture, maintain traceability, and ensure that the docs describe how the repo expects production-grade safety decisions to be made.

## Reference set

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/events-reference.md`](docs/events-reference.md)
- [`docs/adr-001-oracle-adapter.md`](docs/adr-001-oracle-adapter.md)
- [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md)

## Invariants

1. Security invariants stay explicit and visible in the documentation, not implicit in code.
2. Privileged surfaces remain deny-by-default without implicit trust for any caller.
3. Traceability is preserved through structured logs, correlation ids, and stable error codes.
4. Money-path or mainnet-affecting operations remain behind a pause, kill-switch, or documented rollback path.
5. Event and operational docs stay aligned with the contract’s real security model and contributor expectations.

## Completion summary

- The repo-level docs maintain the security narrative that every privileged change is intentional, authorized, and observable.
- Error semantics, event schemas, and upgrade guidance stay connected so operators can trace outcomes safely.
- Contributor documentation reinforces the operational requirement that no money-path change lands without a safety control or rollback plan.

## Acceptance criteria record

- [x] authz and fail-closed model documented
- [x] correlation-id and observability model captured
- [x] security and event references aligned
- [x] upgrade and rollback guidance cross-linked
- [x] kill-switch / feature-flag guidance retained

## Status

Archived as complete within the repo’s full hardening sequence.
