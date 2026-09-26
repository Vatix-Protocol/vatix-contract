# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Vatix Protocol, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, please email security@vatix.io with:

- A description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested remediation

We will acknowledge receipt within 48 hours and provide a timeline for a fix.

## Scope

This policy covers all packages in the Vatix-Protocol monorepo, including:

- `vatix-contract` — Soroban smart contracts
- `vatix-sdk` — client SDKs
- `vatix-indexer` — off-chain indexing services
- `vatix-api` — backend APIs

## Upgrade Security

Multi-contract upgrades are a privileged, money-path operation. The exact
upgrade order, invariants, preconditions, and rollback steps are documented in
[`scripts/upgrade/UPGRADE_PLAYBOOK.md`](scripts/upgrade/UPGRADE_PLAYBOOK.md).

Key security invariants for upgrades:

- Dependencies are upgraded before dependents; never the reverse.
- Storage version compatibility is verified before any write.
- Admin/authz roles are preserved across upgrades.
- Upgrades are fail-closed: missing env, wrong network, or failed preflight
  aborts the run.

All upgrade scripts must be run by an authorized operator against the intended
network. Untrusted clients cannot bypass upgrade policy.

## Supported Versions

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |
| < latest | ❌       |

## Disclosure Policy

We follow coordinated disclosure. Once a fix is available, we will publish a
security advisory and credit the reporter (unless anonymity is requested).
