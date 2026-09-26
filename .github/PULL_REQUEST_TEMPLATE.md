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

## Test plan

<!-- Unit / integration / e2e coverage, plus manual Freighter checklist when needed. -->

- [ ] Unit tests for invariants and auth negatives
- [ ] Integration/e2e on the critical path
- [ ] CI green

## Rollback / flag strategy

<!-- How is this reverted or disabled if it misbehaves? Reference the flag/kill-switch. -->

## Contributor docs

- [ ] README and [CONTRIBUTING.md](../CONTRIBUTING.md) cross-links updated where relevant.
