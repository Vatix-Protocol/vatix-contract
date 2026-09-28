# ADR-001: Oracle Adapter — Collusion Assumptions and Trust Model

- **Status:** Accepted
- **Issue:** #946 (Oracle collusion assumptions documented)
- **Scope:** `vatix-contract` oracle adapter used by `contracts/market` and
  `contracts/resolution` for price/outcome resolution.

## Context

The market and resolution contracts consume prices and outcomes from an oracle
adapter. If the adapter's trust assumptions are undocumented, contributors may
over-trust a single source, silently fall back to a stale price, or assume the
adapter is Byzantine-fault-tolerant when it is not. This ADR records the
collusion assumptions and the fail-closed invariants the adapter must uphold.

## Trust Model

- **Trusted roles.** Only the configured admin may add, remove, or reweight
  oracle sources and update adapter config. Oracle sources are identified by
  their authorized key/role; any other caller is rejected (`NotAdmin` /
  `Unauthorized`) before any state is written.
- **Untrusted inputs.** Source reports are untrusted data. The adapter validates
  signer authorization, freshness, and bounds before a report can influence a
  money path. A report from an unauthorized key is discarded, not averaged in.
- **No secrets.** Source keys, RPC URLs, and credentials never appear in the
  repository or in logs; only redacted identifiers are emitted.

## Collusion Assumptions

1. **Quorum.** A price is accepted only when at least the configured minimum
   number of distinct authorized sources agree within the deviation bound. A
   single source (or fewer than quorum) can never move a money path.
2. **Distinctness.** Quorum counts distinct authorized sources; duplicate
   reports from the same source (including replayed reports) count once.
3. **Majority collusion is out of scope for safety, in scope for detection.**
   If a majority of authorized sources collude, they can produce a
   within-bounds price. The adapter does **not** claim to prevent this; instead
   it bounds the blast radius: deviation and staleness limits cap how far a
   colluding quorum can move a price, and every accepted price is observable so
   operators can detect and pause.
4. **No silent fallback.** A colluding or failing quorum cannot cause the
   adapter to fall back to a last-known price on a money path; the dependent
   write reverts instead.

## Fail-Closed Invariants

- Writes that depend on an oracle price **revert** when the price is stale,
  missing, below quorum, or outside the configured deviation band. There is no
  silent fallback to the last-known price on money paths.
- Staleness is measured against the configured max age; a report older than the
  bound is treated as missing.
- Deviation is measured against the configured band; a report outside the band
  is rejected rather than clamped.
- Dependency outages (RPC/DB/Redis) fail closed on writes: the adapter does not
  proceed with partial or cached state.
- Replayed or concurrent reports are idempotent: a report already applied does
  not double-count toward quorum or move the price twice.

## Authorization

- Deny-by-default for every privileged surface: only the stored admin may
  change oracle config or sources; only authorized oracle roles may submit
  reports.
- Auth expiry or wrong role aborts before any state-affecting step.
- Untrusted clients cannot bypass policy; the contract remains the source of
  truth for prices, balances, swaps, and admin.

## Observability

- Every accepted price and every rejection (stale, missing, below quorum,
  out-of-band, unauthorized) emits an event with a correlation id for alerting.
- Metrics cover money paths (accepted/rejected prices, quorum size, staleness
  age) without leaking secrets.

## Rollback / Kill-Switch

- Oracle-dependent money paths are gated by the per-contract pause kill-switch
  (see `SECURITY.md` → *Pause Trading*). Pausing the market blocks
  `update_position`, `resolve_market`, and related writes while leaving
  read-only views available.
- Rollback is `unpause` with no state migration; a collusion incident is
  handled by pausing, rotating sources, and resuming.

## References

- `SECURITY.md` — security principles, pause kill-switch, threat model.
- `docs/SECURITY.md` — oracle report verification path (#898).
- `docs/threat-model.md` — trust boundaries and mitigations.
