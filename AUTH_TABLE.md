# Authorization Table — Vatix Market Contract

This document is the source of truth for **who may call which privileged surface** in
`contracts/market`. It exists so that Stellar Wave contributors, integrators, and
operators can audit guardian/admin powers without reading the whole contract.

> Invariant: **deny-by-default**. Any entrypoint not listed here as callable by a
> given role MUST fail closed with a stable error code. There is no implicit
> superuser; the admin is the only role that can grant or revoke other roles.

## Roles

| Role | Description | How it is assigned |
| --- | --- | --- |
| `Admin` | Root authority. Owns role grants/revocations and money-path configuration. | Set at `initialize`; transferable only via `transfer_admin` (two-step). |
| `Guardian` | Safety role. May pause/unpause and trigger the kill-switch, but cannot move funds or change configuration. | Granted/revoked by `Admin` via `set_guardian`. |
| `Operator` | Routine, non-privileged maintenance (e.g. oracle refresh). | Granted/revoked by `Admin` via `set_operator`. |
| `Public` | Any untrusted caller. | Implicit. |

## Privileged surfaces

| Entrypoint | Admin | Guardian | Operator | Public | Notes |
| --- | :---: | :---: | :---: | :---: | --- |
| `initialize` | — | — | — | ✅ (once) | Reverts `AlreadyInitialized` on replay. |
| `transfer_admin` | ✅ | ❌ | ❌ | ❌ | Two-step: propose then `accept_admin`. |
| `accept_admin` | — | — | — | ✅ (pending only) | Only the pending admin; reverts `NotPendingAdmin`. |
| `set_guardian` | ✅ | ❌ | ❌ | ❌ | Emits `GuardianChanged`. |
| `set_operator` | ✅ | ❌ | ❌ | ❌ | Emits `OperatorChanged`. |
| `pause` | ✅ | ✅ | ❌ | ❌ | Idempotent; emits `Paused`. |
| `unpause` | ✅ | ✅ | ❌ | ❌ | Idempotent; emits `Unpaused`. |
| `set_kill_switch` | ✅ | ✅ | ❌ | ❌ | Fail-closed on writes while engaged. |
| `set_oracle` | ✅ | ❌ | ❌ | ❌ | Money-path config; behind feature flag. |
| `set_fee_config` | ✅ | ❌ | ❌ | ❌ | Money-path config; behind feature flag. |
| `refresh_oracle` | ✅ | ✅ | ✅ | ❌ | Non-privileged maintenance. |
| `open_position` / `close_position` / `transfer_position` / `merge_positions` | ✅ | ✅ | ✅ | ✅ | User-facing; subject to pause/kill-switch. |

## Stable error codes

| Code | Name | Meaning |
| --- | --- | --- |
| `E100` | `Unauthorized` | Caller is not the required role. |
| `E101` | `NotPendingAdmin` | `accept_admin` called by a non-pending address. |
| `E102` | `AlreadyInitialized` | `initialize` replayed. |
| `E103` | `Paused` | Write attempted while paused. |
| `E104` | `KillSwitchEngaged` | Write attempted while kill-switch is engaged. |
| `E105` | `InvalidRole` | Role grant/revoke with an unknown or zero address. |
| `E106` | `Replayed` | Idempotency key already consumed. |

## Transparency & observability

Every privileged action emits a structured event carrying the acting address, the
new state, and a correlation/idempotency identifier so off-chain observers can
audit authority changes:

- `AdminTransferProposed`, `AdminTransferred`
- `GuardianChanged`, `OperatorChanged`
- `Paused`, `Unpaused`, `KillSwitchToggled`
- `OracleUpdated`, `FeeConfigUpdated`

Read-only transparency entrypoints expose the current admin, guardian, operator,
and pending-admin addresses plus the paused/kill-switch flags, so integrators can
verify authority without trusting an off-chain indexer.

## Edge cases

- **Replay / concurrency**: privileged calls are idempotent where noted; replayed
  idempotency keys revert `E106`.
- **Dependency outage**: writes fail closed (`E103`/`E104`) rather than proceeding
  on stale oracle/RPC state.
- **Auth expiry / wrong role**: reverts `E100`; no partial state mutation.
- **Adversarial input**: zero/invalid addresses revert `E105`.
- **Testnet vs mainnet**: admin/guardian addresses are network-specific; never
  reuse a testnet key on mainnet. Verify addresses against this table per network.

## Related

- `SECURITY.md` — disclosure policy and threat model.
- `docs/adr-001-oracle-adapter.md` — oracle trust assumptions.
