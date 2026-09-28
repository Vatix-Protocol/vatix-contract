# Settlement Idempotency & Implementation Summary

This document is the canonical reference for the settlement idempotency
contract in `vatix-contract` and the invariants that back the
`IMPLEMENTATION_SUMMARY` archive/sync path. It is written for Stellar Wave
contributors and must stay in sync with the on-chain entrypoints, error
codes, and authz rules described below.

## Scope

- Package: `vatix-contract`
- Surface: settlement entrypoints, admin/privileged entrypoints, and the
  archive/sync path referenced by `IMPLEMENTATION_SUMMARY.md`.
- Goal: make the implementation summary match the actual contract behavior
  (entrypoints, error codes, authz, idempotency, fail-closed writes).

## Invariants

1. **Source of truth.** The contract is the sole source of truth for
   balances, swaps, settlement state, and admin configuration. Off-chain
   services (indexers, RPC, DB, Redis) are caches/read models only and MUST
   NOT be treated as authoritative for writes.
2. **Deny-by-default.** Every privileged surface (admin, config, upgrade,
   pause/kill-switch) denies by default. A caller must present an explicit
   authorization (admin role / signed auth) for each privileged entrypoint.
3. **Idempotent settlement.** Settlement operations are keyed by a stable
   idempotency key. Replaying the same key MUST NOT double-apply effects;
   the second call returns the original result (or a typed
   `AlreadySettled`-style error) without mutating state.
4. **Fail-closed writes.** If a required dependency (RPC/DB/Redis) is
   unavailable or returns an ambiguous result, writes MUST fail closed:
   no partial state is committed and the caller receives a typed error.
5. **No secrets.** No private keys, tokens, or credentials are stored in
   the repo, in logs, or in emitted events.

## Entrypoints & error codes

- Settlement entrypoints accept an idempotency key and validate it before
  any state mutation.
- Privileged entrypoints require an explicit admin/authz check before
  executing; unauthorized callers receive a typed authorization error.
- Error codes are stable and typed (e.g. authorization failure, invalid
  input, already-settled/replay, dependency-unavailable). Callers should
  branch on the typed error, not on string messages.
- Where applicable, responses carry a correlation id so ops can trace a
  request across logs without leaking secrets.

## Edge cases & failure modes

- **Concurrent / replayed requests.** Two concurrent calls with the same
  idempotency key must serialize; exactly one applies, the other observes
  the settled result. Replays after completion are no-ops.
- **Dependency outage (RPC/DB/Redis).** Writes fail closed. Reads may
  degrade to cached/read-only data but MUST NOT be used to authorize a
  write.
- **Auth expiry / wrong role.** Expired or wrong-role credentials are
  rejected with a typed authorization error; no state changes occur.
- **Adversarial input / griefing.** Inputs are validated and bounded
  before use; malformed or oversized inputs are rejected early. Rate
  limiting and authorization gate every external entrypoint.
- **Testnet vs mainnet / address drift.** Contract addresses and network
  parameters are configuration, not hard-coded assumptions. Any
  mainnet-affecting change is gated behind a feature flag / kill-switch
  and documented with a rollback plan in the PR.

## Security considerations

- The contract remains the source of truth for balances, swaps, and admin.
- No secrets in the repo or logs; events and metrics are scrubbed.
- Every external entrypoint is rate-limited and authorized.
- New privileged surfaces are deny-by-default.
- Money-path changes are feature-flagged or kill-switched, with rollback
  documented in the PR description.

## Observability

- Emit actionable metrics on money paths (settlement attempts, replays,
  authorization failures, dependency-unavailable failures) without
  leaking secrets or PII.
- Logs include correlation ids to trace a request end-to-end.

## Archive / sync notes

- `IMPLEMENTATION_SUMMARY.md` is the human-readable summary; this document
  is the normative idempotency/settlement reference. When they disagree,
  the contract code and this document win, and the summary must be
  updated to match.
- Keep the summary and this reference cross-linked so contributors can
  find the invariants from either entrypoint.

## Test plan

- Unit tests for invariants and auth negatives (wrong role, expired auth,
  unauthorized privileged calls).
- Idempotency tests: concurrent same-key calls, replay after completion.
- Fail-closed tests: dependency outage on the write path.
- Integration/e2e on the critical settlement path.
- CI stays green; add a required check if the path is ungated.
- Manual Freighter checklist when a wallet interaction is involved.

## Acceptance criteria

- [ ] Behavior matches the cited docs and the actual contract.
- [ ] Authz / idempotency / fail-closed covered by tests.
- [ ] Docs and runbooks updated; mainnet safety respected.
- [ ] Observability actionable; metrics on money paths.
- [ ] Rollback / flag strategy documented in the PR.
