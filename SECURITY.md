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

## Security Principles

- The server/contract is the source of truth for balances, swaps, and admin actions. Clients are never trusted for state.
- Deny-by-default for every privileged surface: new entrypoints must authorize explicitly and fail closed.
- No secrets in the repository or in logs. Environment values, keys, and RPC URLs must be redacted before being emitted.
- Every external entrypoint is rate-limited and authorized.
- Money-path and mainnet-affecting changes land behind a feature flag or kill-switch with a documented rollback.

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
  caller fails with `NotAdmin`/`Unauthorized` before the flag is written.
- **Fail closed:** paused state blocks writes; read-only views stay available
  so users and operators can inspect balances during an incident.
- **Idempotent:** pausing an already-paused contract (or unpausing an active
  one) is a harmless re-write of the same flag.
- **Observable:** every toggle emits an event (`emergency_pause_toggled` on the
  market, `contract_paused`/`contract_unpaused` on outcome-token, treasury
  paused/unpaused events on the treasury) for alerting.
- **Rollback:** `unpause` restores normal operation with no state migration.

For a coordinated protocol-wide stop, pause each contract (market first) and
keep the mirrored `EmergencyMode` in sync (Issue #662).

## Error Codes and Storage Layouts

- Stable per-contract error codes: [`docs/error-codes.md`](docs/error-codes.md).
- Resolution storage keys and invariants: [`docs/resolution-storage.md`](docs/resolution-storage.md).

## Staging Dry-Run Checklist (Automated)

The staging dry-run is automated by `scripts/upgrade/staging_dry_run.sh`, which executes the steps described in
[`scripts/upgrade/STAGING_DRY_RUN_CHECKLIST.md`](scripts/upgrade/STAGING_DRY_RUN_CHECKLIST.md).

Security-relevant guarantees of the automated dry-run:

- **Fail-closed:** the script exits non-zero if any check fails or is missing. A dry-run that cannot verify a step is treated as a failure.
- **Deny-by-default authz:** the caller's admin/role and the target network are verified before any state-affecting step. Wrong role or expired auth aborts the run.
- **Mainnet guard:** the script refuses to run against mainnet unless the explicit readiness flag (`VATIX_MAINNET_READY=1`) is set. Testnet is the default target.
- **Idempotency:** concurrent or replayed invocations are serialized via a run lock and a run id, so a dry-run cannot be applied twice.
- **Dependency outage:** RPC/DB outages fail closed on writes; the script does not proceed with partial state.
- **No secret leakage:** environment values, keys, and RPC URLs are redacted from stdout and logs.
- **Address drift:** testnet vs mainnet addresses are validated against the expected network before use.

See the checklist for the full list of steps and the runbook for rollback instructions.

## Freighter Wallet Integration

The browser wallet integration is documented in
[`docs/freighter-integration-guide.md`](docs/freighter-integration-guide.md).
Freighter is an **untrusted client**: it holds signing keys and proposes
signatures, but it is never authoritative for protocol state.

- The contract remains the **source of truth** for balances, swaps, and admin.
  Wallet-reported balances, network, and addresses are display hints only and
  must be re-verified against the contract before any money-path action.
- Every signed transaction is **authorized on-chain**; a signature from a
  connected wallet does not by itself grant a role or bypass policy. Wrong-role
  or expired-auth submissions are rejected by the contract, not the client.
- Writes **fail closed** when the RPC is unavailable or the wallet returns an
  unexpected network/address. Never treat a client-side success as settlement.
- **No secrets** are stored in the repo or logs. Freighter keys never leave the
  extension; the app must not log signed XDR, keys, or raw signatures.
- Testnet vs mainnet **address drift** is a security concern: verify the
  connected network and contract id before signing, and refuse to sign when they
  do not match the configured deployment.

All upgrade scripts must be run by an authorized operator against the intended
network. Untrusted clients cannot bypass upgrade policy.

## Implementation Summary

The 

/* … truncated 1829 chars — edit only what you need near the top … */
