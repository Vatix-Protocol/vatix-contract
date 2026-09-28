# Threat Model — Prediction Markets (#933)

Scope: `contracts/market`, `contracts/outcome-token`, `contracts/resolution`,
`contracts/treasury`. Policy: [`SECURITY.md`](../SECURITY.md).
Authz reference: [`AUTH_TABLE.md`](../AUTH_TABLE.md).

## Trust boundaries

| Actor | Trust | Notes |
| --- | --- | --- |
| Admin | Privileged | Only role that can set fees, waivers, oracle and contract wiring. |
| Oracle / resolver | Semi-trusted | Outcome accepted only via signed/adapter path; challengeable in `resolution`. |
| Users / wallets (Freighter) | Untrusted | Every entrypoint calls `require_auth`; client state is never authoritative. |
| Outcome-token contract | Trusted dependency | Mint/burn only callable by the market. |

## Threats and mitigations

| Threat | Mitigation | Reference |
| --- | --- | --- |
| Spoofed admin / fee-waiver self-grant | `require_auth` before admin equality; waivers read from storage only | `contracts/market/src/fee_waiver.rs` |
| Unbounded list griefing | `MAX_FEE_WAIVERS` cap → `FeeWaiverCapReached` | `fee_waiver.rs` |
| Double settlement / replay | `is_settled` persisted before external calls (CEI); replay → `PositionAlreadySettled` | `contracts/market/src/settlement.rs`, `SETTLEMENT_IDEMPOTENCY.md` |
| Losing tokens left live after settlement | Both YES and NO sides burned on every settle path | `settlement.rs` (`burn_settled_outcome_tokens`) |
| Position/token ledger divergence | `assert_position_token_parity` blocks settlement until admin reconcile | `contracts/market/src/reconciliation.rs` |
| Reentrancy on money paths | Checks-effects-interactions ordering | `docs/reentrancy-cei-audit.md` |
| Oracle manipulation / wrong outcome | Adapter-verified signatures, challenge window | `docs/adr-001-oracle-adapter.md` |
| Fee rounding / overflow | Floor division, `checked_mul` → `ArithmeticOverflow` | `docs/dust-handling.md` |
| Unsafe upgrade / storage drift | Version check, ordered multi-contract upgrade, fail-closed CI | `scripts/upgrade/UPGRADE_PLAYBOOK.md` |
| Network / address drift | Deploy scripts verify network and contract ids; mainnet gated by `VATIX_MAINNET_READY=1` | `SECURITY.md` |
| Secret leakage | No keys in repo; scripts redact env, keys and RPC URLs | `SECURITY.md` |

## Failure posture

- **Fail closed:** any failed auth, parity, or arithmetic check aborts the
  transaction; no partial state is committed.
- **Deny by default:** new privileged entrypoints must be added to
  `AUTH_TABLE.md` with an explicit role check.
- **Observability:** money-path state changes emit events
  (`docs/events-reference.md`) that carry no secrets.

## Rollback

Documentation only; no on-chain behavior change. Revert the commit to roll back.
