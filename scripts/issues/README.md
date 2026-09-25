# Issue Scripts Quality Bar

This directory holds the operational scripts used to triage, reproduce, and
verify GitHub issues for the Vatix contracts. This document is the canonical
quality bar every script in `scripts/issues/` must meet before it is merged.

## Scope

Applies to every executable entrypoint under `scripts/issues/` (shell, Rust
binaries, or Node helpers) that reads or mutates protocol state, calls an RPC,
or touches a database/Redis. Read-only, purely local helpers may skip the
money-path sections but must still honor the secrets and error-code rules.

## Invariants

1. **Idempotency.** Re-running a script with the same inputs must be safe.
   Replayed or concurrent invocations must not double-apply a write. Writes
   carry a caller-supplied `--idempotency-key` (or derive one from the issue
   id + action) and the target must reject a duplicate key with
   `E_DUPLICATE_REQUEST` rather than re-executing.
2. **Fail-closed on dependency outage.** If the RPC, database, or Redis is
   unreachable, times out, or returns an ambiguous result, any write path
   must abort with a non-zero exit and `E_DEPENDENCY_UNAVAILABLE`. Never
   proceed on a best-effort basis for money-path or mainnet-affecting writes.
3. **Deny-by-default authz.** Privileged surfaces (admin actions, settlement,
   treasury, resolution finalization) require an explicit role/credential.
   Absent or expired auth fails closed with `E_UNAUTHORIZED` / `E_AUTH_EXPIRED`.
   Untrusted clients cannot bypass policy by omitting a flag.
4. **No secrets in repo or logs.** Never commit keys, tokens, or mnemonics.
   Read them from the environment at runtime and redact them from all output.
   Logs must never echo secret values, full signed payloads, or raw auth headers.

## Error codes

Scripts emit stable, machine-readable error codes so CI and runbooks can
branch on them. Codes are part of the public contract; do not renumber.

| Code | Meaning |
|---|---|
| `E_INVALID_INPUT` | Malformed or adversarial input rejected before any side effect |
| `E_UNAUTHORIZED` | Missing or wrong role for a privileged surface |
| `E_AUTH_EXPIRED` | Credential present but expired |
| `E_DUPLICATE_REQUEST` | Idempotency key already applied |
| `E_DEPENDENCY_UNAVAILABLE` | RPC/DB/Redis outage or timeout on a write path |
| `E_NETWORK_MISMATCH` | Testnet vs mainnet / address drift detected |
| `E_INTERNAL` | Unexpected failure; safe to retry only if idempotent |

## Correlation ids

Every entrypoint accepts `--correlation-id <id>` (or generates a UUIDv4 when
omitted) and includes it in every log line, metric label, and error payload.
The id is propagated to downstream RPC/DB calls so a single issue run can be
traced end to end. Correlation ids are opaque and must not encode secrets.

## Observability

- Emit ops-safe structured logs (JSON) with `correlation_id`, `issue_id`,
  `action`, `outcome`, and `error_code`.
- Money-path scripts expose counters for attempts, successes, and failures
  keyed by `error_code`, plus a latency histogram. Labels must never contain
  secrets, addresses beyond the public contract id, or user PII.
- On failure, log the failing step and the error code; never log the secret
  or the full request body.

## Network safety

- Scripts must state the target network explicitly and refuse to run against
  mainnet unless `--allow-mainnet` is passed and the readiness checklist in
  the PR is satisfied.
- Detect address drift between testnet and mainnet and fail with
  `E_NETWORK_MISMATCH` before any write.
- Any money-path or mainnet-affecting change lands behind a feature flag or
  kill-switch, with rollback documented in the PR description.

## Checklist for new scripts

- [ ] Idempotent writes with `E_DUPLICATE_REQUEST` on replay.
- [ ] Fail-closed on RPC/DB/Redis outage for writes.
- [ ] Deny-by-default authz on privileged surfaces.
- [ ] Stable error codes and correlation ids on every entrypoint.
- [ ] Ops-safe metrics/logs; no secrets in repo or logs.
- [ ] Network guard and mainnet flag documented.
- [ ] Unit tests for invariants and auth negatives; integration/e2e on the
      critical path; CI stays green.
