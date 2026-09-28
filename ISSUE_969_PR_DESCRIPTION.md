# PR for #969: Hardening follow-up 7

## Summary

This final hardening follow-up preserves the repo’s production-readiness posture by documenting the remaining authz, observability, and contributor guidance gaps. The result is a clear, explicit contract: deny-by-default authorization, safe traceability, and a documented emergency stop for any money-path or mainnet-affecting change.

## References

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/events-reference.md`](docs/events-reference.md)
- [`docs/adr-001-oracle-adapter.md`](docs/adr-001-oracle-adapter.md)
- [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md)

## Invariants

- privileged behavior stays explicit and deny-by-default
- logs and events remain safe and operationally useful
- stable errors and correlation ids support incident diagnosis
- pause and kill-switch controls remain available for money-path safety
- upgrade / rollback decisions are documented and discoverable

## Why this is required

Without this final pass, contributors and operators can still infer the wrong security model from partial docs or stale assumptions. This cleanup keeps the institutional knowledge in the repo and aligns it with the actual fail-closed design.

## Checklist

- [x] authz model preserved
- [x] observability guidance retained
- [x] docs cross-linking updated
- [x] security references validated
- [x] rollback and kill-switch guidance included
