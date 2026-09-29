# ISSUE_966 Archive: Hardening follow-up 4

> Status: archived — issue #966 resolved as docs/ops hardening and invariants documentation.

## Acceptance criteria record

- [x] Design note references the authz/rollback/security docs and states the hardening invariants.
- [x] Authz and fail-closed security posture preserved and documented.
- [x] Correlation-id / observability guidance linked from the repo-level docs.
- [x] README and SECURITY cross-links added for contributors and operators.
- [x] Mainnet/money-path change guidance includes the kill-switch requirement.

## Changes summary

### Canonical references

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`apps/web/lib/contracts/README.md`](apps/web/lib/contracts/README.md)
- [`scripts/issues/README.md`](scripts/issues/README.md)

### Completed work

1. Confirmed the deny-by-default authz posture for privileged entrypoints.
2. Documented the kill-switch and fail-closed pause guarantees.
3. Recorded the correlation-id and ops-safe logging pattern for money-path events.
4. Cross-linked the documentation from contributor-facing docs and security policies.
5. Confirmed that mainnet-affecting changes remain behind a documented rollback or kill-switch path.

## Resolution note

This issue is closed as a documentation and operational hardening task. The repository already contains the security-critical authority and observability model, and the follow-up work is to make that model explicit and durable for contributors operating the protocol.
