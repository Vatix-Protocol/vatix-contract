# Security Policy

## Reporting a Vulnerability

Please report suspected vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" under the repository's **Security** tab) or by emailing
the maintainers listed in `CODEOWNERS`. Do not open a public issue for security
reports. We aim to acknowledge reports within 72 hours.

## Scope

This policy covers the `vatix-contract` package and the localnet deploy path used
by contributors. It does not cover third-party dependencies or the public Stellar
networks themselves.

## Invariants

These invariants must hold on every network, including localnet:

- The contract is the **source of truth** for balances, swaps, and admin state.
  Clients (including the deploy scripts) never compute or cache authoritative
  balances.
- Privileged surfaces are **deny-by-default**: admin/initialization entrypoints
  require an explicit authorized signer and reject unauthenticated callers.
- Writes **fail closed** when a dependency (RPC/DB/Redis) is unavailable; a
  failed dependency must never
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
  inaccuracy could lead to a mistaken security assumption

These invariants must hold on every network, including localnet:

- The contract is the **source of truth** for balances, swaps, and admin state.
  Clients (including the deploy scripts) never compute or cache authoritative
  balances.
- Privileged surfaces are **deny-by-default**: admin/initialization entrypoints
  require an explicit authorized signer and reject unauthenticated callers.
- Writes **fail closed** when a dependency (RPC/DB/Redis) is unavailable; a
  failed dependency must never be treated as success.
- **No secrets** are committed to the repository or written to logs. Deploy
  scripts read keys from the environment or a local keystore only.

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
