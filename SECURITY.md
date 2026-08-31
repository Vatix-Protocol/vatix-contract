# Security Policy

Vatix Protocol manages user funds through on-chain Soroban smart contracts.
We take vulnerability reports seriously and ask that they be reported
**privately**, not through public GitHub issues.

## Scope

This policy covers the smart contracts in this repository
(`Vatix-Protocol/vatix-contract`):

- `contracts/market` — market creation, trading, deposits, settlement
- `contracts/treasury` — protocol fee custody and distribution
- `contracts/resolution` — challenge-based outcome resolution
- `contracts/outcome-token` — per-market YES/NO outcome tokens
- Deployment/upgrade tooling under `scripts/` (e.g. `scripts/upgrade/`)
- Documentation that describes on-chain invariants (`AUTH_TABLE.md`,
  `docs/adr-001-oracle-adapter.md`, `docs/reentrancy-cei-audit.md`) where an
  inaccuracy could lead to a mistaken security assumption

Out of scope: the `apps/web` frontend's UI/UX bugs that don't touch contract
calls or key handling, third-party dependencies (report those upstream), and
issues requiring physical access to a user's device or already-compromised
keys.

## Reporting a Vulnerability — do NOT open a public issue

**If you believe you have found a security vulnerability — especially one
that is exploitable against funds, admin authorization, or contract
upgrades — do not open a public GitHub issue, pull request, or discussion
describing it.** Public disclosure before a fix is deployed can let an
attacker exploit it against mainnet funds before we can respond.

Instead, report it privately through one of:

1. **GitHub Security Advisories (preferred):** open a private advisory via
   this repository's **Security** tab → "Report a vulnerability". This
   creates a private channel visible only to maintainers until we jointly
   agree to disclose.
2. **Email:** `security@vatix.example` — if possible, encrypt sensitive
   details (e.g. proof-of-concept exploit code) and note that you'd like a
   PGP key if we don't have one on file yet.

Please include, as available:

- A description of the vulnerability and its potential impact (funds at
  risk, contract(s) affected, privilege level required).
- Steps to reproduce, or a minimal proof-of-concept (test case, script, or
  transaction trace).
- The commit hash / tag / deployed contract address you tested against.
- Whether the issue is already being exploited in the wild.

## Our commitment (SLA)

- **Acknowledgment:** we will acknowledge receipt of your report within
  **48 hours**.
- **Triage:** we will provide an initial severity assessment and expected
  timeline within **5 business days** of acknowledgment.
- **Fix timeline (target, may vary with complexity):**
  - Critical (direct loss/theft of funds, admin takeover, consensus
    bypass): fix or mitigation within **7 days**.
  - High (fund loss requiring specific preconditions, DoS of core flows):
    within **14 days**.
  - Medium/Low: within **30 days**, or scheduled into the next regular
    release.
- **Disclosure:** we will coordinate public disclosure with you once a fix
  is deployed (or a mitigation is in place), and are happy to credit
  reporters who wish to be named.

## Non-critical issues

Bugs that are not security-sensitive (typos, non-exploitable logic errors,
test flakiness, documentation gaps) are welcome as normal public GitHub
issues — this private-reporting requirement applies specifically to
exploitable vulnerabilities.

## Bounty

We do not currently run a formal bug bounty program. We may offer
discretionary rewards for high-quality reports of critical/high-severity
issues; this will be discussed directly with the reporter once a report is
triaged.
