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

## Localnet Deploy

The contributor localnet deploy path (build, deploy, initialize, smoke-verify)
is documented in [`CONTRIBUTING.md`](./CONTRIBUTING.md#localnet-deploy-contributor-path).
Contributors must:

- Use throwaway keys generated for localnet only; never reuse testnet or mainnet
  keys on a local network, and never commit them.
- Treat localnet as untrusted: the same authz and fail-closed rules that apply to
testnet/mainnet apply locally, so the path exercises the real policy.
- Keep the deploy behind the documented feature flag/kill-switch when it touches
  any money path, and record the rollback steps in the PR description.

## Mainnet Safety

Irreversible mainnet changes require the readiness checklist and are out of scope
for the localnet contributor path. Do not point localnet tooling at mainnet
endpoints or keys.
