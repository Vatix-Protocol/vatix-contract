# Contributing to Vatix Protocol

Thanks for contributing to the Vatix-Protocol monorepo. This guide covers the
baseline expectations for every package, with a dedicated section for the
`vatix-contract` issue scripts.

## Getting started

1. Fork the repository and create a topic branch off `main`.
2. Keep changes scoped to a single issue; avoid unrelated refactors.
3. Run the package's test suite locally before opening a PR.
4. Open a PR that links the issue it resolves and describes the rollback plan
   for any money-path or mainnet-affecting change.

## General expectations

- Match existing patterns, types, and module structure.
- Never commit secrets, tokens, or credentials.
- Keep CI green; add a required check if a new surface is ungated.
- Update docs and runbooks when behavior changes.

## Issue scripts quality bar (`scripts/issues/`)

Scripts under `scripts/issues/` are operational tooling that can touch
money-path state. They must meet the bar below before merge. See
[`scripts/issues/README.md`](scripts/issues/README.md) for the full reference.

### Invariants

- **Idempotency.** Replayed or concurrent runs must be safe. Every write
  entrypoint takes a stable idempotency key and must not double-apply effects.
- **Fail-closed writes.** On RPC/DB/Redis outage, writes abort with a typed
  error rather than partially applying. Reads may degrade; writes must not.
- **Deny-by-default authz.** Privileged surfaces require an explicit role and
  reject untrusted callers. New privileged entrypoints start denied.
- **No secrets.** Never log or commit secrets, keys, or tokens. Redact
  sensitive fields in logs and metrics.

### Error codes and correlation ids

- Use stable, documented error codes for script entrypoints; do not reuse a
  code for a different failure mode.
- Propagate a correlation id through every entrypoint and include it in logs
  and error responses so operators can trace a run end to end.

### Observability

- Emit ops-safe metrics and logs on money paths (counts, latencies, outcomes)
  without leaking secrets or user-identifying data.
- Metrics must be actionable: alert on write failures and authz denials.

### Safety

- Feature-flag or kill-switch any money-path or mainnet-affecting change.
- Document the rollback strategy in the PR description.
- Respect testnet vs mainnet address drift; never hardcode mainnet addresses
  in scripts without an explicit, reviewed flag.

## Pull request checklist

- [ ] Behavior matches the cited docs for the issue.
- [ ] Authz, idempotency, and fail-closed behavior covered by tests.
- [ ] Docs/runbooks updated; mainnet safety respected.
- [ ] Observability is actionable; metrics on money paths.
- [ ] Rollback/flag strategy documented in the PR description.
