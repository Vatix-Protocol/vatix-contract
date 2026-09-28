# Resolution Contract Storage Keys (Issue #905)

Source of truth: [`contracts/resolution/src/storage.rs`](../contracts/resolution/src/storage.rs).
All keys live in **persistent** storage under the `StorageKey` enum.

| Key                           | Value type             | Written by                                   | Description                                                                                                       |
| ----------------------------- | ---------------------- | -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `StorageVersion`              | `u32`                  | `initialize`                                 | Schema version guard. Must equal `STORAGE_VERSION` (currently `1`) or mutating calls fail with `UpgradeRequired`. |
| `Config`                      | `ResolutionConfig`     | `initialize`, admin setters                  | Admin, market/factory addresses, bond and window parameters.                                                      |
| `CandidateCounter`            | `u32`                  | `propose`                                    | Monotonic candidate id allocator; never decremented or reused.                                                    |
| `Candidate(u32)`              | `ResolutionCandidate`  | `propose`, `challenge`, `appeal`, `finalize` | Candidate state keyed by candidate id.                                                                            |
| `CandidateByMarket(u32)`      | `u32`                  | `propose`                                    | Market id → candidate id index; enforces one candidate per market (`CandidateAlreadyExists`).                     |
| `ProposerCollateral(Address)` | `i128`                 | collateral deposit/withdraw, `propose`       | Proposer bond collateral balance. Defaults to `0` when absent.                                                    |
| `Challengers(u32)`            | `Vec<ChallengeRecord>` | `challenge`, `appeal`, `finalize`            | Bonded challengers for a candidate, bounded by `MAX_APPEAL_ROUNDS + 1`; cleared on finalize.                      |
| `Treasury`                    | `Address`              | treasury timelock execute                    | Optional recipient of slashed-bond treasury share. Unset → share stays in the contract.                           |
| `PendingTreasury`             | `PendingAddressChange` | propose/cancel/execute treasury              | Timelocked treasury rotation (Issue #687).                                                                        |
| `PendingFactory`              | `PendingAddressChange` | propose/cancel/execute factory               | Timelocked factory rotation.                                                                                      |
| `PendingMarketContract`       | `PendingAddressChange` | propose/cancel/execute market contract       | Timelocked market contract rotation.                                                                              |
| `EmergencyMode`               | `EmergencyMode`        | admin emergency setter                       | Mirrored coordinated emergency mode (Issue #662).                                                                 |
| `Paused`                      | `bool`                 | `pause` / `unpause`                          | When `true`, `propose`, `challenge`, `appeal`, and `finalize` fail with `ContractPaused`. Absent → `false`.       |

## Invariants

- **Fail closed on version drift:** every state-mutating entrypoint calls
  `assert_version` before touching storage; a missing or mismatched
  `StorageVersion` returns `UpgradeRequired` instead of reading a stale layout.
- **One candidate per market:** `CandidateByMarket` is written in the same call
  as `Candidate`, so the index and record cannot diverge. Replayed `propose`
  calls for the same market are rejected.
- **Ids are never reused:** `CandidateCounter` only increments.
- **Timelocked rotations:** `Treasury`, factory, and market contract changes
  proposed through `propose_*` wait in their `Pending*` key until the delay
  elapses; the pending entry is removed on execute or cancel. Note that
  `set_market_contract` is an admin-only direct setter on `Config` that
  bypasses the timelock.
- **Deny-by-default:** only the stored admin may write `Config`, `Paused`,
  `EmergencyMode`, or any `Pending*` key.

## Changing the layout

Renaming, reordering, or changing the value type of any key is a breaking
change: bump `STORAGE_VERSION`, add a version-history entry in `storage.rs`,
update this table, and follow
[`scripts/upgrade/UPGRADE_PLAYBOOK.md`](../scripts/upgrade/UPGRADE_PLAYBOOK.md).
