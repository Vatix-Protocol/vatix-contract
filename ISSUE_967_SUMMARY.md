# Issue #967: Hardening follow-up 5 - residual authz/observability/docs gaps

## Overview

This issue resolves the residual hardening follow-up for the next pass over authz, traceability, and contributor documentation. The code and docs already follow a deny-by-default security model; this archive codifies the invariants and operational links that keep that model intact.

## Reference set

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`docs/threat-model.md`](docs/threat-model.md)
- [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md)

## Invariants

1. Authz is explicit: no privileged entrypoint can be treated as implicitly authorized.
2. Fail closed: missing auth, missing network checks, or stalled dependency paths stop write execution.
3. Correlation ids are traceable and safe: they help operators follow a request end-to-end without exposing secrets.
4. Observability remains ops-safe: logs and metrics are structured and sanitized for production usage.
5. Rollback stays available: pause/kill-switch and feature-gated launch paths remain the default emergency control.

## Completion summary

- The repo-level security and auth documentation clearly states that privileged surfaces must be deny-by-default.
- Error-code guidance and upgrade playbooks preserve the stable contract semantics needed for incident triage.
- Threat-model and security docs document operator expectations without leaking secrets or internal trust assumptions.
- Follow-up documentation keeps the repo aligned with the fail-closed standard for money-path and mainnet-affecting updates.

## Acceptance criteria record

- [x] authz invariant documented
- [x] error-code and traceability contract documented
- [x] security and runbook links added
- [x] observability pattern includes safe logging and metrics
- [x] feature-flag / kill-switch guidance included
- [x] contributor-facing docs remain synced

## Status

Archived as complete within the repository hardening model.
