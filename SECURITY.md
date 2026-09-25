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
  failed dependency must never be treated as success.
- **No secrets** are committed to the repository or written to logs. Deploy
  scripts read keys from the environment or a local keystore only.

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
