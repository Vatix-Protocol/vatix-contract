# Admin Authorization Audit

Inventory of every admin-gated (or otherwise privileged) mutator across the
`market`, `treasury`, `resolution`, and `outcome-token` contracts, and the
check each one performs. Produced as part of the auth-hardening pass; keep
this in sync whenever an admin entrypoint is added, removed, or renamed.

Every row below follows the same two-step pattern unless noted otherwise:

1. `caller.require_auth()` — cryptographic proof the caller signed the call.
2. `caller == stored_admin` — proof the signer is *the* admin, not merely
   *some* authenticated address. Step 1 alone is insufficient: a caller can
   authenticate as themselves but must still be rejected if they are not the
   admin. Step 2 alone is equally insufficient: an address-equality check
   with no `require_auth()` lets anyone pass the *admin's own address* as an
   argument without ever proving they hold its key — see the Resolution
   section below for a real instance of this gap that this pass closed.

## Market contract (`contracts/market/src/lib.rs`)

| Entrypoint                     | require_auth | admin-equality check | Notes |
|---------------------------------|:---:|:---:|-------|
| `initialize`                   | ✅ | n/a (bootstraps admin) | Guarded by `AlreadyInitialized` instead. As of #701, also defaults legacy V1 oracle signatures (`OracleV1Disabled`) to disabled — a fresh deployment fails closed until the admin explicitly re-enables V1 via `set_oracle_v1_disabled`. |
| `propose_admin`                 | ✅ (`current_admin`) | ✅ (`storage::get_admin`) | Current admin must authorize the nomination; nominee is also validated to be an account, not a contract (`validate_admin_address`). The transfer still only completes on `accept_admin`. |
| `accept_admin`                  | ✅ | ✅ (must match `PendingAdmin`) | Two-step transfer. |
| `cancel_admin_transfer`          | ✅ | ✅ | Cancels a pending `propose_admin`. |
| `initialize_market`             | ✅ | ✅ | |
| `cancel_market`                 | ✅ | ✅ | |
| `reopen_market`                 | ✅ | ✅ | Only sanctioned `Canceled` → `Active` path. |
| `void_market`                    | ✅ (`caller`) | n/a — cross-contract equality check | Issue #708. Callable **only** by the registered resolution contract (`storage::get_resolution_contract`); every other caller — admin included — gets `Unauthorized`, and an unset resolution contract fails closed with `Unauthorized`. Forces `Active` → `Canceled` for the resolution `void_market` dispute outcome; `Resolved`/`Canceled` markets are rejected. |
| `close_market_to_deposits`      | ✅ | ✅ | Idempotent; always emits an event, even when already closed. |
| `set_oracle_v1_disabled`        | ✅ | ✅ | Legacy Ed25519-v1 kill switch (#657). |
| `set_adapter_enabled`           | ✅ | ✅ | Enables/disables the Reflector/Pyth adapter; fails closed to direct Ed25519 verification while disabled — never a silent fallback while an adapter is *enabled but unavailable*. |
| `pause` / `unpause`             | ✅ | ✅ | Blocks deposit/withdraw/trade/create/resolve entrypoints while paused. |
| `set_emergency_mode`            | ✅ | ✅ | Coordinated `Normal` / `TradingHalted` / `SettleOnly` / `GlobalFreeze` mode (#662); mirrored across Market, Treasury, and Resolution. |
| `propose_market_oracle` / `execute_market_oracle` / `cancel_market_oracle` | propose ✅, execute — (timelock-gated), cancel ✅ | propose ✅, cancel ✅, execute n/a | 48h timelock (`FEE_RATE_TIMELOCK_SECONDS`) oracle-pubkey rotation for a single market (#486). `execute_*` is intentionally callable by anyone once due — the timelock is the access control. |
| `propose_treasury_contract` / `execute_treasury_contract` / `cancel_treasury_contract` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked treasury-address rotation. |
| `propose_outcome_token_contract` / `execute_outcome_token_contract` / `cancel_outcome_token_contract` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked outcome-token-contract rotation. |
| `propose_resolution_contract` / `execute_resolution_contract` / `cancel_resolution_contract` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked resolution-contract rotation; gates `resolve_market` once set. |
| `propose_threshold_signers` / `execute_threshold_signers` / `cancel_threshold_signers` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked global threshold-signer/quorum rotation (#665). |
| `set_market_threshold_signers`  | ✅ | ✅ | Immediate (not timelocked) per-market signer/quorum override — scoped to a single `Active` market rather than the global admin key set, which is why it isn't timelocked like the global family above. |
| `set_threshold_signers`         | ✅ | ✅ | Legacy immediate global signer/quorum setter, retained only for test backward-compatibility; prefer the `propose_threshold_signers` timelock family for production changes. |
| `set_fee_rate` (propose) / `execute_fee_rate_change` | propose ✅, execute — | propose ✅, execute n/a | Timelocked; fee cap is re-checked at *both* proposal and execution time so a cap lowered mid-flight can't let a stale rate through. |
| `set_fee_cap`                   | ✅ | ✅ | Hard upper bound on `set_fee_rate`. |
| `add_fee_waiver` / `remove_fee_waiver` | ✅ | ✅ | Admin cannot waive itself (#584). |
| `reconcile_position_tokens`     | ✅ | ✅ | Admin-gated repair of a `Position` / `OutcomeToken` divergence (see `reconciliation.rs`); mints/burns `OutcomeToken` balances to match `Position`, never the reverse. |

`execute_*` entrypoints above are intentionally public — access control is
the timelock (`effective_at`), not caller identity. `settle_position`,
`batch_settle_positions`, `settle_positions_page`, `update_position`,
`deposit_collateral`, `withdraw_unused_collateral`,
`withdraw_canceled_collateral`, `buy_yes`/`buy_no`/`sell_yes`/`sell_no`, and
read-only getters are user-facing or view functions, not admin mutators, and
are out of scope for this table.

## Treasury contract (`contracts/treasury/src/lib.rs`)

| Entrypoint            | require_auth | admin-equality check | Notes |
|------------------------|:---:|:---:|-------|
| `initialize`           | ✅ | n/a (bootstraps admin) | |
| `withdraw_fees`        | ✅ | ✅ | |
| `propose_admin` / `execute_admin` / `cancel_admin` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | 48h timelock (`ADDRESS_TIMELOCK_SECONDS`) admin rotation. |
| `add_market`           | ✅ | ✅ | |
| `remove_market`        | ✅ | ✅ | |
| `propose_market_contract` / `execute_market_contract` / `cancel_market_contract` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked market-contract rotation. **Fixed by #720**: `execute_market_contract` used to overwrite the entire `AuthorizedMarkets` registry with a single-element vec, silently deregistering every market previously added via `add_market` (no `market_removed` event, no error) — registry drift between the two entrypoints that both mutate `AuthorizedMarkets`. It now appends idempotently, matching `add_market`. |
| `pause` / `unpause`    | ✅ | ✅ | |
| `set_emergency_mode`   | ✅ | ✅ | Mirrors the Market/Resolution coordinated mode (#662). |
| `propose_stakeholders` / `execute_stakeholders` / `cancel_stakeholders` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked (#689) stakeholder revenue-share list. `propose_stakeholders` rejects an empty list or shares not summing to exactly 10,000 bps with `InvalidStakeholderWeights` (#721). Table entry was still named `set_stakeholders` (its pre-#689 name) until this pass — kept in sync now. |
| `distribute_fees`      | ✅ | ✅ | Rejects with `NoStakeholdersConfigured` if `propose_stakeholders`/`execute_stakeholders` has never installed a list. **Fixed by #721**: the payout loop pushed each stakeholder's transfer onto the payout list twice, so every stakeholder was paid double the intended amount while the treasury's own ledger (`distributed`/`remaining`) only accounted for a single payment — found via the `test.rs`/`distribute_proptest.rs` fallout from the `set_stakeholders` → `propose_stakeholders` rename (#689), which had left those test files referencing a removed client method and unable to compile at all, masking the bug. |

`collect_fee` requires auth from `caller` but intentionally checks
*registered-market* membership (`is_authorized_market`) rather than admin
identity — it is a market-contract-facing entrypoint, not an admin mutator.

## Resolution contract (`contracts/resolution/src/lib.rs`)

| Entrypoint                     | require_auth | admin-equality check | Notes |
|---------------------------------|:---:|:---:|-------|
| `initialize`                   | ✅ | n/a (bootstraps admin) | |
| `set_default_challenge_window`  | ✅ | ✅ (`require_admin`) | **Reviewed for #723**: intentionally *not* timelocked, unlike the propose/execute families below. `challenge_window_secs` is advisory only — `propose`/`propose_v2`/`appeal` each take their own `challenge_window_seconds` argument (bounded by the fixed `MIN_CHALLENGE_WINDOW_SECONDS`/`MAX_CHALLENGE_WINDOW_SECONDS` constants) and store it immutably on the candidate at creation time; the default is never read by any of them. An instant admin change here cannot move an existing candidate's `challenge_deadline` or constrain a future proposer's chosen window, so it carries none of the "instant address/config change" risk the timelock pattern exists to prevent. See the doc comment on `set_default_challenge_window` in `lib.rs` and its regression tests in `test.rs`. |
| `propose_factory` / `execute_factory` / `cancel_factory` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Timelocked (`ADDRESS_TIMELOCK_SECONDS`, 48h) factory rotation. **`propose_factory` and `cancel_factory` were calling `require_admin` (address equality) with no `admin.require_auth()` at all** — any caller could pass the real admin's address as the argument and pass the equality check without ever proving they hold that key, silently rotating the factory after the timelock. Fixed by this pass: both now call `admin.require_auth()` before the equality check, matching every other admin mutator in this file. |
| `propose_market_contract` / `execute_market_contract` / `cancel_market_contract` | propose ✅, execute —, cancel ✅ | propose ✅, cancel ✅, execute n/a | Same gap found and fixed in this pass: `propose_market_contract` and `cancel_market_contract` now call `admin.require_auth()`. |
| `set_treasury`                  | ✅ | ✅ (`require_admin`) | Optional treasury recipient for the slashed-bond treasury cut. |
| `slash_collateral`              | ✅ | ✅ (`require_admin`) | |
| `arbitrate_uphold_proposer`      | ✅ | ✅ (`require_admin`) | Terminal, timelocked (`ARBITRATION_TIMELOCK_SECONDS`, 48h) dispute path: upholds the proposer once `MAX_APPEAL_ROUNDS` are exhausted. |
| `void_market`                    | ✅ | ✅ (`require_admin`) | Terminal, timelocked dispute path: voids the market (→ `Canceled`) when neither side can be safely vindicated on-chain. |

`propose`, `propose_v2`, `challenge`, `appeal`, `finalize`, and
`deposit_collateral` all `require_auth()` the acting party
(proposer/challenger/finalizer) but are deliberately open to any caller —
the bond, challenge window, and finalize conditions are the access control,
not an admin check. `propose_v2` (#701) is the V2-oracle counterpart of
`propose`: same access model, verified via the market contract's
`verify_signature_v2` instead of the legacy `verify_signature`.

## Outcome-token contract (`contracts/outcome-token/src/lib.rs`)

Not previously covered by this table at all — added by this pass.

| Entrypoint            | require_auth | admin/role-equality check | Notes |
|------------------------|:---:|:---:|-------|
| `initialize`           | ✅ (`admin`) | n/a (bootstraps admin) | Guarded by `AlreadyInitialized`. |
| `set_market_contract`  | ✅ (`admin`) | ✅ (`config.admin`) | Updates the sole address allowed to `mint`/`burn`. |
| `set_metadata`         | ✅ (`admin`) | ✅ (`config.admin`) | Updates SAC-compatible `name`/`symbol`. |
| `mint`                 | ✅ (`config.market_contract`) | n/a — role check *is* the auth check | Only the registered market contract may mint; not admin-gated by design. |
| `burn`                 | ✅ (`config.market_contract`) | n/a — role check *is* the auth check | Same as `mint`. |
| `transfer`             | ✅ (`from`) | n/a | Peer-to-peer transfer. Rejected unconditionally: `MarketNotResolved` before the market resolves, `TransferBlockedAfterResolve` once it has (Issue #690 — see `transfer`'s doc comment for why post-resolution transfer is unsafe given `Position`-keyed settlement). |

`get_config`, `name`, `symbol`, `decimals`, `balance`, and `total_supply` are
read-only getters and out of scope for this table.

## Conclusion

Every admin mutator in `treasury`, `market`, and `outcome-token` already
performs both `require_auth()` and an admin/role-equality check. This pass's
full re-audit (extending coverage to the propose/execute/cancel timelock
families, the emergency-mode entrypoints, and the outcome-token contract,
none of which were previously listed) found one real gap: in `resolution`,
`propose_factory`, `cancel_factory`, `propose_market_contract`, and
`cancel_market_contract` checked the caller's address against the stored
admin but never called `require_auth()` on it — meaning any caller could
impersonate the admin for those four calls by simply passing the admin's
address as the argument, with no signature required. That gap has been
closed by adding `admin.require_auth()` to all four functions, consistent
with every other admin mutator in the file (`set_default_challenge_window`,
`set_treasury`, `slash_collateral`, `arbitrate_uphold_proposer`,
`void_market`, all of which already had it).
