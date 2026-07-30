# Treasury Contract — Storage Layout

> **Keep this table in sync with `contracts/treasury/src/storage.rs`.**  
> Bump `STORAGE_VERSION` in that file whenever the layout changes in a
> breaking way (field added/removed, type changed, semantic change).

## Current storage version: `2`

### Version history

| Version | Change |
|---------|--------|
| **v2** | Completed the multi-market `AuthorizedMarkets` registry (`add_market` / `remove_market` / `list_markets` / `is_authorized_market`) and added the `Stakeholders` fee-distribution list (#485). |
| **v1** | Initial storage layout. |

---

## StorageKey enum

| Key | Storage tier | Value type | Description |
|-----|-------------|-----------|-------------|
| `StorageVersion` | `instance` | `u32` | Written by `initialize`; guards against stale or uninitialized deployments. Every accessor calls `assert_version` before reading data. |
| `Admin` | `instance` | `Address` | The address that may call `withdraw_fees` and other admin-only operations. Set once at initialization; transferable via `transfer_admin`. |
| `AuthorizedMarkets` | `instance` | `Vec<Address>` | The set of market contract addresses allowed to call `collect_fee`. Managed via `add_market` / `remove_market`. Returns an empty list when unset (not an error). |
| `TokenBalance(Address)` | `persistent` | `i128` | Current custodied balance for a specific collateral token. Increases on `collect_fee`, decreases on `withdraw_fees` / `distribute_fees`. Key parameter: token mint address. |
| `CumulativeFees(Address)` | `persistent` | `i128` | Monotonically increasing historical total of all fees ever collected for a token. Never decreases — useful for off-chain accounting and audit trails. Key parameter: token mint address. |
| `TotalCollected` | `instance` | `i128` | Global monotone counter: sum of all fees ever collected across every token. Never decreases. |
| `Paused` | `instance` | `bool` | When `true`, `collect_fee` and `withdraw_fees` are blocked until an admin calls `unpause`. Defaults to `false` when unset. |
| `Stakeholders` | `instance` | `Vec<(Address, u32)>` | Ordered list of `(stakeholder_address, share_bps)` pairs used by `distribute_fees` (#485). All `share_bps` values must sum to exactly `10_000`. Empty list when `set_stakeholders` has never been called. |
| `FeeTokens` | `instance` | `Vec<Address>` | Registry of every distinct token mint that has ever had a fee routed through `collect_fee` (#484). Lets callers enumerate which tokens hold a balance without prior knowledge of token addresses. Append-only and idempotent — re-registering an already-known token is a no-op. |

---

## Notes

- **Instance vs persistent storage**: `instance`-tier keys share the contract
  instance's TTL and are cheaper to access. `persistent`-tier keys have their
  own TTL that can be extended independently — used here for per-token
  balances because they must survive across arbitrary time spans.
- **Version guard**: `assert_version(env)` is called at the start of every
  data accessor. If the on-chain `StorageVersion` does not match the compiled
  constant, every read returns `TreasuryError::UpgradeRequired`, preventing
  silent data corruption after an upgrade without a migration.
- **Fee token registry**: `FeeTokens` is the canonical enumeration of all
  tokens the treasury has ever handled. Use `get_fee_tokens()` to iterate
  balances instead of tracking token addresses off-chain.
