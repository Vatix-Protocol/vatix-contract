# Treasury Contract — Storage Layout

> **Keep this table in sync with `contracts/treasury/src/storage.rs`.**  
> Bump `STORAGE_VERSION` in that file whenever the layout changes in a
> breaking way (field added/removed, type changed, semantic change).

## Current storage version: `3`

### Version history

| Version | Change |
|---------|--------|
| **v3** | Added `EmergencyMode` for coordinated emergency mode mirrored with Market/Resolution (#662). **Note (#722):** the `StorageKey::EmergencyMode` variant was referenced by `get_emergency_mode`/`set_emergency_mode` but was never actually added to the `StorageKey` enum when this version was cut — the crate did not compile from that commit until it was fixed by #722. The version number and this changelog entry were correct in intent from the start; only the enum itself had drifted from what the version history already documented. |
| **v2** | Completed the multi-market `AuthorizedMarkets` registry (`add_market` / `remove_market` / `list_markets` / `is_authorized_market`) and added the `Stakeholders` fee-distribution list (#485). |
| **v1** | Initial storage layout. |

---

## StorageKey enum

| Key | Storage tier | Value type | Description |
|-----|-------------|-----------|-------------|
| `StorageVersion` | `instance` | `u32` | Written by `initialize`; guards against stale or uninitialized deployments. Every accessor calls `assert_version` before reading data. |
| `Admin` | `instance` | `Address` | The address that may call `withdraw_fees` and other admin-only operations. Set once at initialization; transferable via the timelocked `propose_admin` / `execute_admin` / `cancel_admin` (#658). |
| `AuthorizedMarkets` | `instance` | `Vec<Address>` | The set of market contract addresses allowed to call `collect_fee`. Managed via `add_market` / `remove_market`. Returns an empty list when unset (not an error). |
| `TokenBalance(Address)` | `persistent` | `i128` | Current custodied balance for a specific collateral token. Increases on `collect_fee`, decreases on `withdraw_fees` / `distribute_fees`. Key parameter: token mint address. |
| `CumulativeFees(Address)` | `persistent` | `i128` | Monotonically increasing historical total of all fees ever collected for a token. Never decreases — useful for off-chain accounting and audit trails. Key parameter: token mint address. |
| `TotalCollected` | `instance` | `i128` | Global monotone counter: sum of all fees ever collected across every token. Never decreases. |
| `Paused` | `instance` | `bool` | When `true`, `collect_fee` and `withdraw_fees` are blocked until an admin calls `unpause`. Defaults to `false` when unset. |
| `Stakeholders` | `instance` | `Vec<(Address, u32)>` | Ordered list of `(stakeholder_address, share_bps)` pairs used by `distribute_fees` (#485). All `share_bps` values must sum to exactly `10_000`; an empty or non-summing list is rejected at `propose_stakeholders` time with `InvalidStakeholderWeights` (#721). Set via the timelocked `propose_stakeholders` / `execute_stakeholders` / `cancel_stakeholders` (#689). Empty list when no stakeholder change has ever executed — `distribute_fees` rejects with `NoStakeholdersConfigured` in that case. |
| `FeeTokens` | `instance` | `Vec<Address>` | Registry of every distinct token mint that has ever had a fee routed through `collect_fee` (#484). Lets callers enumerate which tokens hold a balance without prior knowledge of token addresses. Append-only and idempotent — re-registering an already-known token is a no-op. |
| `EmergencyMode` | `instance` | `EmergencyMode` | Coordinated emergency mode mirrored with the Market/Resolution contracts (#662): `Normal` / `TradingHalted` / `SettleOnly` / `GlobalFreeze`. Only `GlobalFreeze` blocks `collect_fee` / `withdraw_fees` / `distribute_fees` on this contract. Defaults to `Normal` when unset. Added by #722 — see the v3 note above. |

---

## Reviewer Checklist: StorageKey Table Drift

Three independent descriptions of storage must always agree, and nothing in
the compiler enforces the third:

1. The **`StorageKey` enum** in `src/storage.rs` — the actual, compiled
   source of truth for what can be written to storage.
2. The **`## Storage layout` doc table** in the crate-level `//!` doc comment
   at the top of `src/lib.rs`.
3. **This document** — the human-readable reference for reviewers,
   integrators, and off-chain indexers.

Check all three on every PR that touches `src/storage.rs` or adds/removes a
stored type:

- [ ] **List every `StorageKey` variant.** Open `src/storage.rs` and read the
      full `pub enum StorageKey { ... }` block.
- [ ] **Grep for every `StorageKey::` reference across the whole crate** —
      not just the enum and the two doc tables. `#722` was a variant
      (`EmergencyMode`) that two accessor functions referenced but that was
      never declared in the enum at all; diffing the enum against the doc
      tables alone would not have caught it, because both tables were
      *also* missing it — the drift was between code-that-compiles and
      code-that-doesn't, not between two prose descriptions. A working
      compiler catches this immediately (`cargo check -p vatix-treasury-contract`
      fails with "no variant named `EmergencyMode`"), but only if CI is
      actually gating merges on it — see the note below.
- [ ] **List every row in both doc tables** (`src/lib.rs`'s `## Storage
      layout` and this file's `## StorageKey enum`) and diff them against
      the enum by hand. Every enum variant must have exactly one
      corresponding row in each table, and vice versa.
- [ ] **Check the `Type`/`Value type` and `Description` columns**, not just
      variant names — a field-type change on a variant (e.g. `Address` ->
      `Vec<Address>`) should also update both tables.
- [ ] **Cross-check against `STORAGE_VERSION` bumps.** If the version
      history here (or the `## Version history` comment above
      `STORAGE_VERSION` in `storage.rs`) claims a version added a key,
      confirm that key actually exists in the enum — `#722`'s v3 entry
      described `EmergencyMode` correctly from the start; only the enum
      itself lagged behind its own changelog.
- [ ] **Run `cargo check -p vatix-treasury-contract --all-targets` locally
      before opening the PR.** This is the actual enforcement mechanism for
      the enum-variant-must-exist class of drift; the checklist above is
      for the two prose tables, which the compiler cannot check for you.

**Known example this checklist caught:** `EmergencyMode` was referenced by
`get_emergency_mode`/`set_emergency_mode` in `src/lib.rs` since #662 but was
never added to the `StorageKey` enum, and separately, `PendingMarketContract`,
`PendingAdmin`, and `PendingStakeholders` existed in the enum but were
missing from this document's table entirely — both fixed by #722. Treat any
future mismatch the same way: fix the enum/tables/version-history together
in the same PR, don't defer it.

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
