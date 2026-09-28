# Pull Request

## Summary

<!-- What does this PR change and why? Link the issue(s) it closes. -->

Closes #

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] Security / disclosure
- [ ] Refactor (no behavior change)

## Security checklist

- [ ] No secrets, keys, or tokens are committed (repo or logs).
- [ ] Server/contract remains the source of truth for balances, swaps, and admin actions.
- [ ] New privileged surfaces are deny-by-default and authorized.
- [ ] External entrypoints are rate-limited and authorized.
- [ ] Money-path or mainnet-affecting changes are behind a feature flag / kill-switch.
- [ ] Fail-closed behavior on dependency outage (RPC/DB/Redis) is preserved for writes.
- [ ] Disclosure process followed — see [SECURITY.md](../SECURITY.md) for reporting channels and supported versions.

## Authorization (AUTH_TABLE)

<!-- Required when this PR adds, removes, renames, or changes the auth checks of
any contract entrypoint (anything that calls `require_auth()`, or any admin /
role / timelock gate). CI job "AUTH_TABLE coverage (#892)" runs
`bash scripts/check-auth-table.sh` and fails on missing or stale entries. -->

- [ ] No entrypoint was added, removed, renamed, or re-gated in this PR, **or**
- [ ] [AUTH_TABLE.md](../AUTH_TABLE.md) is updated in this PR: each changed entrypoint lists its `require_auth` subject, admin/role-equality check, and timelock/kill-switch notes.
- [ ] New privileged entrypoints are deny-by-default (`require_auth()` **and** an admin/role-equality check) and have an auth-negative test.
- [ ] `bash scripts/check-auth-table.sh` passes locally.

## Test plan

<!-- Unit / integration / e2e coverage, plus manual Freighter checklist when needed. -->

- [ ] Unit tests for invariants and auth negatives
- [ ] Integration/e2e on the critical path
- [ ] CI green — all required status checks pass (see [CONTRIBUTING.md](../CONTRIBUTING.md#required-ci-status-checks) for the branch-protection check names)

## Rollback / flag strategy

<!-- How is this reverted or disabled if it misbehaves? Reference the flag/kill-switch. -->

## Contributor docs

- [ ] README and [CONTRIBUTING.md](../CONTRIBUTING.md) cross-links updated where relevant.
