# Security Policy

## Reporting a Vulnerability

Please report suspected vulnerabilities privately to the maintainers (do not open a public issue). Include a description, impact, and reproduction steps. We aim to acknowledge reports within 72 hours.

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for the contributor workflow and
[`README.md`](./README.md) for the project overview and supported versions.

## Supported Versions

Security fixes are provided for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Scope

This policy covers the Vatix-Protocol monorepo, including `vatix-contract` (Soroban contracts), the market/settlement paths, and the operational tooling under `scripts/`.

## Hardening follow-up archives

The follow-up issue archives document the repo’s residual authz, observability, and documentation hardening work for contributors and operators:

- [COMPLETE_ISSUE_966.md](COMPLETE_ISSUE_966.md)
- [COMPLETE_ISSUE_967.md](COMPLETE_ISSUE_967.md)
- [COMPLETE_ISSUE_968.md](COMPLETE_ISSUE_968.md)
- [COMPLETE_ISSUE_969.md](COMPLETE_ISSUE_969.md)

## Security Principles

- The server/contract is the source of truth for balances, swaps, and admin actions. Clients are never trusted for state.
- Deny-by-default for every privileged surface: new entrypoints must authorize explicitly and fail closed.
- No secrets in the repository or in logs. Environment values, keys, and RPC URLs must be redacted before being emitted.
- Every external entrypoint is rate-limited and authorized.
- Money-path and mainnet-affecting changes land behind a feature flag or kill-switch with a documented rollback.

## Hardening Follow-up 1: Residual Authz / Observability / Docs Gaps (Issue #963)

This section closes the residual gaps left after the initial hardening pass. It
is the normative summary for the invariants below; the per-entrypoint detail
lives in [`AUTH_TABLE.md`](AUTH_TABLE.md) and the settlement replay rules live
in [`SETTLEMENT_IDEMPOTENCY.md`](SETTLEMENT_IDEMPOTENCY.md).

### Deny-by-default authz

- Every privileged entrypoint authorizes **before** any state read or write.
  A caller that is not the stored admin (or an explicitly granted role) fails
  with `NotAdmin`/`Unauthorized` and no state is mutated.
- New privileged surfaces are denied by default: they must be added to
  [`AUTH_TABLE.md`](AUTH_TABLE.md) (CI-enforced by
  `scripts/check-auth-table.sh`, #892) before they can be called.
- Wrong role or expired auth aborts the call; there is no implicit fallback to
  a less-privileged path.

### Fail-closed on dependency outage

- Writes fail closed when a dependency (RPC/DB/Redis) is unavailable. A write
  that cannot verify its preconditions is rejected rather than partially
  applied.
- Reads may degrade, but a degraded read never authorizes a write.
- No silent fallback to a last-known value on money paths (see the oracle
  assumptions below).

### Idempotency and replay handling

- Settlement and other money-path writes are idempotent: concurrent or
  replayed requests are serialized and applied at most once. The exact keys
  and replay rules are documented in
  [`SETTLEMENT_IDEMPOTENCY.md`](SETTLEMENT_IDEMPOTENCY.md).
- Replayed oracle reports count once toward quorum; a replayed settlement is a
  no-op, not a second transfer.

### Observability without secret leakage

- Money paths emit actionable metrics and events (e.g. pause toggles, fee
  collection, settlement outcomes) so operators can alert and pause.
- Logs and metrics redact secrets: environment values, keys, and RPC URLs are
  never emitted. Correlation ids are used to trace a request without exposing
  credentials.

### Rollback / kill-switch

- Any money-path or mainnet-affecting change lands behind a feature flag or the
  per-contract pause kill-switch (see *Pause Trading* below) with a documented
  rollback. No state migration is required to roll back a pause.

## Hardening Follow-up 8: Residual Authz / Observability / Docs Gaps (Issue #970)

This section closes the residual gaps left after follow-up 1 (#963). It is the
normative summary for the invariants below; the per-entrypoint authz detail
lives in [`AUTH_TABLE.md`](AUTH_TABLE.md), the settlement replay rules live in
[`SETTLEMENT_IDEMPOTENCY.md`](SETTLEMENT_IDEMPOTENCY.md), and the project
overview lives in [`README.md`](README.md).

### Deny-by-default for new privileged surfaces

- The server/contract remains the **source of truth** for balances, swaps, and
  admin actions. Clients are never trusted for state.
- Every new privileged surface is **denied by default**: it must authorize
  explicitly and be registered in [`AUTH_TABLE.md`](AUTH_TABLE.md)
  (CI-enforced by `scripts/check-auth-table.sh`, #892) before it can be called.
- A caller that is not the stored admin (or an explicitly granted role) fails
  with `NotAdmin`/`Unauthorized` and no state is mutated. Wrong role or expired
  auth aborts the call; there is no implicit fallback to a less-privileged path.

### Idempotency and replay handling

- Settlement and other money-path writes are idempotent: concurrent or
  replayed requests are serialized and applied **at most once**. The exact keys
  and replay rules are documented in
  [`SETTLEMENT_IDEMPOTENCY.md`](SETTLEMENT_IDEMPOTENCY.md).
- Replayed oracle reports count once toward quorum; a replayed settlement is a
  no-op, not a second transfer.

### Fail-closed on dependency outage

- Writes fail closed when a dependency (RPC/DB/Redis) is unavailable. A write
  that cannot verify its preconditions is rejected rather than partially
  applied.
- Reads may degrade, but a degraded read never authorizes a write, and there is
  no silent fallback to a last-known value on money paths.

### Observability without secret leakage

- Money paths emit actionable metrics and events (e.g. pause toggles, fee
  collection, settlement outcomes) so operators can alert and pause.
- Logs and metrics redact secrets: environment values, keys, and RPC URLs are
  never emitted. Correlation ids trace a request without exposing credentials.
- No secrets in the repository or in logs.

### Rollback / kill-switch

- Any money-path or mainnet-affecting change lands behind a feature flag or the
  per-contract pause kill-switch (see *Pause Trading* below) with a documented
  rollback. No state migration is required to roll back a pause.

## Oracle Collusion Assumptions (Issue #946)

The oracle adapter's trust model and collusion assumptions are documented in
[`docs/adr-001-oracle-adapter.md`](docs/adr-001-oracle-adapter.md). Key points:

- Only the configured admin may change oracle config or sources; only
  authorized oracle roles may submit reports. Deny-by-default for new
  privileged surfaces.
- A price is accepted only at or above the configured quorum of **distinct**
  authorized sources agreeing within the deviation band; replayed reports count
  once.
- **Majority collusion is not prevented** — it is bounded and detected. A
  colluding quorum cannot exceed the deviation/staleness bounds, and every
  accepted price is observable so operators can pause.
- **Fail closed:** oracle-dependent writes revert on stale, missing,
  below-quorum, or out-of-band prices. There is no silent fallback to a
  last-known price on money paths.
- Rollback is the per-contract pause kill-switch (see *Pause Trading* below);
  no state migration is required.

## Upgrade Security

Multi-contract upgrades are a privileged, money-path operation. The exact
upgrade order, invariants, preconditions, and rollback steps are documented in
[`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md).

In-scope components:

- `contracts/market` — market creation, trading, deposits, settlement
- `contracts/treasury` — protocol fee custody and distribution
- `contracts/resolution` — challenge-based outcome resolution
- `contracts/outcome-token` — per-market YES/NO outcome tokens
- Deployment/upgrade tooling under `scripts/` (e.g. `scripts/upgrade/`)
- Issue/ops scripts under `scripts/issues/` — see
  [`scripts/issues/README.md`](scripts/issues/README.md) for the quality bar
  (idempotency, fail-closed writes, deny-by-default authz, no secrets in
  repo or logs) that these scripts must meet
- Documentation that describes on-chain invariants (`AUTH_TABLE.md`,
  `docs/adr-001-oracle-adapter.md`, `docs/reentrancy-cei-audit.md`) where an
  inaccuracy could lead to a mistaken security assumption. `AUTH_TABLE.md` is
  CI-enforced by `scripts/check-auth-table.sh` (#892); the oracle resolution
  gates are documented in
  [`docs/SECURITY.md`](docs/SECURITY.md#oracle-report-verification-path-898) (#898)

## Threat Model

See [`docs/threat-model.md`](docs/threat-model.md) for trust boundaries and
mitigations, and [`docs/dust-handling.md`](docs/dust-handling.md) for the
fee-rounding dust rule.

## Invariants

These invariants must hold on every network, including localnet:

Key security invariants for upgrades:

- Dependencies are upgraded before dependents; never the reverse.
- Storage version compatibility is verified before any write.
- Admin/authz roles are preserved across upgrades.
- Upgrades are fail-closed: missing env, wrong network, or failed preflight
  aborts the run.

## Pause Trading (Issue #900)

**Decision:** trading pause is implemented as a per-contract, admin-only
kill-switch. There is no separate "trading-only" pause; the existing pause
flag is the single emergency stop for each money path.

| Contract | Entrypoints | Blocked while paused | Error |
| -------- | ----------- | -------------------- | ----- |
| `contracts/market` | `pause` / `unpause` / `is_paused` | `deposit_collateral`, `withdraw_unused_collateral`, `update_position`, `initialize_market`, `cancel_market`, `resolve_market`, `resolve_market_threshold` | `ContractPaused` |
| `contracts/resolution` | `pause` / `unpause` / `is_paused` | `propose`, `challenge`, `appeal`, `finalize` | `ContractPaused` |
| `contracts/treasury` | `pause` / `unpause` / `is_paused` | `collect_fee`, `withdraw_fees`, `distribute_fees` | `ContractPaused` |
| `contracts/outcome-token` | `pause` / `unpause` / `is_paused` | token mutation paths (tokens are non-transferable) | `ContractPaused` |

Invariants:

- **Deny-by-default:** only the stored admin may pause or unpause; any other
  caller fails with `NotAdmin`/`Unauthorized` b