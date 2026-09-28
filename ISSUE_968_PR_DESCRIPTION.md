# PR for #968: Hardening follow-up 6

## Summary

This PR completes the sixth pass of the residual authz / observability / docs hardening work. The outcome is a stricter, clearer model: protocol state remains authoritative on-chain, clients are untrusted, and operational tooling documents how to trace an incident safely.

## References

- [`AUTH_TABLE.md`](AUTH_TABLE.md)
- [`SECURITY.md`](SECURITY.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/freighter-integration-guide.md`](docs/freighter-integration-guide.md)
- [`docs/error-codes.md`](docs/error-codes.md)
- [`scripts/issues/README.md`](scripts/issues/README.md)

## Invariants

- authenticated on-chain checks are the source of truth
- no client-side trust bypass for money-path execution
- stable errors and correlation ids are preserved for analysis
- logs and metrics are safe for production usage
- mainnet or money-path changes stay feature-flagged or kill-switchable

## Why this matters

Operator trust is not the same as protocol truth. This hardening pass preserves the decision boundary between the on-chain contract, the client, and the running infrastructure so a bad wallet, stale dependency, or misconfigured integration cannot silently cross the security boundary.

## Checklist

- [x] authz and network safety documented
- [x] observability requirements preserved
- [x] contributor/operator guidance updated
- [x] wallet/client trust boundaries clarified
- [x] kill-switch / rollback guidance captured
