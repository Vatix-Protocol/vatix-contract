# PR for #966: Hardening follow-up 4

## Summary

This PR closes the residual authz, observability, and documentation gaps for the hardening follow-up track. The work is intentionally documentation-and-operational hardening rather than a typo fix: it preserves the fail-closed posture around privileged entrypoints, writing safety, and operational traceability.

## References

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`apps/web/lib/contract-client.ts`](apps/web/lib/contract-client.ts)
- [`apps/web/lib/contracts/README.md`](apps/web/lib/contracts/README.md)
- [`scripts/issues/README.md`](scripts/issues/README.md)

## Invariants

- Privileged call paths remain deny-by-default and require explicit authorization.
- Pause and kill-switch gates remain authoritative before money-path writes.
- Stable error codes and correlation ids are preserved for incident response and replay analysis.
- Logs remain ops-safe and do not expose secret material.
- Mainnet-affecting or money-path changes remain behind a documented rollback or kill-switch path.

## Why this matters

The hardening path is only complete when the repo teaches contributors and operators what the contract decisions are and how to safely operate them. Without this documentation, security assumptions drift and money-path behaviors become harder to reason about under incident pressure.

## Scope

- authz surface documentation
- observability guidance
- hardening runbook cross-links
- rollback and kill-switch requirements for money-path changes

## Checklist

- [x] deny-by-default authz documented
- [x] stable error codes / correlation ids recorded
- [x] pause / kill-switch invariants documented
- [x] scripts / runbooks cross-linked
- [x] no secrets in logs or repo docs
- [x] mainnet-affecting changes remain feature-gated or kill-switchable
