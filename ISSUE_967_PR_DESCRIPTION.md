# PR for #967: Hardening follow-up 5

## Summary

This change documents and reinforces the remaining hardening follow-up for authz, runbook coverage, and operational traceability. It codifies the repo-level invariants that keep the protocol fail-closed and contributor-safe during upgrades and emergency action.

## References

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`docs/threat-model.md`](docs/threat-model.md)
- [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md)

## Invariants

- no implicit trust for privileged entrypoints
- fail closed on missing or stale dependency state
- correlation ids remain traceable and opaque
- structured logs and metrics must be secret-safe
- money-path changes require a flag or kill-switch path

## Why this is release-safe

A protocol is only as secure as the mental model its operators and contributors share. This change keeps the hardening actions visible in the repo and closes the slight residual gaps between the code path and the operational runbooks.

## Checklist

- [x] authz posture defined
- [x] fail-closed behavior documented
- [x] error-code guidance connected to docs
- [x] observability and logging guidance included
- [x] runbook / security links updated
- [x] kill-switch guidance preserved
