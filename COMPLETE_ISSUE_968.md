# ISSUE_968 Archive: Hardening follow-up 6

> Status: archived — issue #968 resolved as a hardening follow-up covering authz, observability, and contributor docs.

## Acceptance criteria record

- [x] policy on client trust and on-chain authorization captured
- [x] fail-closed operational safeguards retained in doc set
- [x] stable error-code and correlation-id guidance retained
- [x] runbook and issue script cross-links preserved
- [x] kill-switch guidance for money-path change remains explicit

## Changes summary

### Canonical references

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/freighter-integration-guide.md`](docs/freighter-integration-guide.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`scripts/issues/README.md`](scripts/issues/README.md)

### Completed work

1. Emphasized that the contract is the source of truth and clients are untrusted.
2. Kept money-path safety tied to pause, kill-switch, and dependency-failure gotchas.
3. Documented the relationship between error codes, traceability, and incident triage.
4. Linked operational and contributor docs so the hardening posture remains discoverable.

## Resolution note

This issue is closed as a production-readiness and ops documentation follow-up rather than a code defect.
