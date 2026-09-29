# ISSUE_967 Archive: Hardening follow-up 5

> Status: archived — issue #967 resolved as an authz / observability / docs hardening follow-up.

## Acceptance criteria record

- [x] Hardening invariants clearly stated and linked to the security docs.
- [x] Privileged authz model remains fail-closed and explicit.
- [x] Error semantics and traceability documented for operator response.
- [x] Logging and metrics guidance remains code-safe and secret-safe.
- [x] Runbooks and security references remain linked for contributor onboarding.

## Changes summary

### Canonical references

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`docs/threat-model.md`](docs/threat-model.md)
- [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md)

### Completed work

1. Confirmed the deny-by-default authz model and explicit security assumptions.
2. Kept the fail-closed guidance anchored to upgrade and rollback procedures.
3. Linked the error-code and observability guidance to operational docs.
4. Ensured contributor-facing references remain aligned with safe production operating expectations.

## Resolution note

This issue is closed as a follow-up hardening pass aligned to the repository’s fail-closed and security-first model.
