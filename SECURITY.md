# Security Policy

## Reporting a Vulnerability

Please report suspected vulnerabilities privately to the maintainers (do not open a public issue). Include a description, impact, and reproduction steps. We aim to acknowledge reports within 72 hours.

## Scope

This policy covers the Vatix-Protocol monorepo, including `vatix-contract` (Soroban contracts), the market/settlement paths, and the operational tooling under `scripts/`.

## Security Principles

- The server/contract is the source of truth for balances, swaps, and admin actions. Clients are never trusted for state.
- Deny-by-default for every privileged surface: new entrypoints must authorize explicitly and fail closed.
- No secrets in the repository or in logs. Environment values, keys, and RPC URLs must be redacted before being emitted.
- Every external entrypoint is rate-limited and authorized.
- Money-path and mainnet-affecting changes land behind a feature flag or kill-switch with a documented rollback.

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
