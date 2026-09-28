# Issue #966: Hardening follow-up 4 - residual authz/observability/docs gaps

## Overview

This follow-up closes the remaining authz, observability, and documentation gaps identified for the Vatix contract hardening track. The repo already contains the governance and operational artifacts needed to enforce fail-closed behavior and keep contributors aligned on the money-path invariants.

## Reference set

- [`AUTH_TABLE.md`](AUTH_TABLE.md) — canonical admin and auth mapping for privileged entrypoints
- [`SECURITY.md`](SECURITY.md) — security principles, pause/kill-switch behavior, and rollback rules
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor checklist and operational hardening expectations
- [`apps/web/lib/contract-client.ts`](apps/web/lib/contract-client.ts) — correlation-id generation and safe client-side guardrails
- [`apps/web/lib/contracts/README.md`](apps/web/lib/contracts/README.md) — observability guidance and fail-closed money-path patterns
- [`scripts/issues/README.md`](scripts/issues/README.md) — script authoring quality bar for authz and idempotency

## Invariants

1. Deny-by-default authz: privileged entrypoints require explicit role checks and fail closed if the caller is untrusted or missing a required admin condition.
2. Stable errors and traceability: money-path failures surface deterministic error codes and correlation ids so operators can correlate retries and incidents without logging secrets.
3. Fail-closed on safety gates: pause and kill-switch conditions stop privileged writes before state changes are attempted.
4. No secret leakage: correlation ids are opaque, structured logs remain operationally safe, and raw payloads or auth material are never recorded.
5. Feature-flagged rollback: any mainnet-affecting or money-path change remains behind an explicit kill-switch or documented rollback path.

## What this follow-up completes

- Documents the contract-level authorization surface and required admin checks for privileged entrypoints.
- Makes the cross-linkage explicit between `AUTH_TABLE.md`, `SECURITY.md`, `CONTRIBUTING.md`, and the operational automation docs.
- Confirms the observability pattern for write paths: correlation ids, structured logging, and risk-controlled metrics without secret exfiltration.
- Preserves the fail-closed posture on pause/kill-switch and ensures contributors know the rollback path if a money-path change misbehaves.

## Acceptance criteria record

- [x] Authz table updated and linked from the main docs
- [x] Pause / kill-switch behavior documented in security guidance
- [x] Correlation-id and ops-safe observability pattern documented
- [x] Contributors cross-linked to runbooks and security guidance
- [x] Feature-flag / kill-switch guidance included for money-path safety
- [x] No secret-bearing logs or payloads in repo docs

## Status

Completed as a hardening follow-up archive: the repository already carries the fail-closed authz, observability, and rollback model required for this issue class.
