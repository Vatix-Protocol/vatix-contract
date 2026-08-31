# Contract Events Reference

> **Update this table whenever `events.rs` files change.** This is the
> canonical reference for off-chain indexers consuming Vatix contract
> events — keep it in sync with `contracts/*/src/events.rs` whenever an
> event's fields, topics, or emission site change.

Every event below is defined with `#[contractevent]` and published via
`.publish(env)` in the corresponding `emit_*` function. The **Event/Topic**
column is the first (auto-generated) topic in the on-chain event — Soroban's
`#[contractevent]` macro derives it by converting the struct name to
`snake_case` **verbatim** (no suffix stripping), so an event struct's exact
name directly determines its on-chain topic. Every event struct follows the
`{Noun}{Verb}` PascalCase pattern without a redundant `Event` suffix (Issue
#389, audited for consistency across all four contracts in Issue #581), so
e.g. `MarketClosedToDeposits` produces the indexer-friendly topic
`market_closed_to_deposits`, never `market_closed_to_deposits_event`.

In the **Fields** column, fields marked `(topic)` are additional indexed
topics (in declaration order, after the auto-generated event-name topic);
unmarked fields are part of the event's data payload. Many Market contract
events additionally carry a `version: u32 (topic)` field
(`EVENT_VERSION`, currently `1`) as a schema-version topic — see the
"Event schema versioning" note in `contracts/market/src/events.rs`.

## Market (`contracts/market/src/events.rs`)

| Event/Topic | Fields (name: type) | Emitted When |
|---|---|---|
| `contract_initialized` | `version: u32 (topic)`, `admin: Address (topic)`, `initialized_at: u64` | Contract is initialized with an admin (`initialize`) |
| `emergency_pause_toggled` | `version: u32 (topic)`, `paused: bool (topic)`, `timestamp: u64` | The emergency pause flag is toggled on/off |
| `market_created` | `version: u32 (topic)`, `market_id: u32 (topic)`, `creator: Address`, `question: String`, `end_time: u64`, `metadata_uri: Option<String>` | A new prediction market is created |
| `collateral_deposited` | `version: u32 (topic)`, `user: Address (topic)`, `market_id: u32 (topic)`, `amount: i128`, `new_total: i128` | A user deposits collateral into a market |
| `collateral_withdrawn` | `version: u32 (topic)`, `user: Address (topic)`, `market_id: u32 (topic)`, `amount: i128`, `new_total: i128` | A user withdraws collateral from a market |
| `withdraw_edge_case` | `version: u32 (topic)`, `user: Address (topic)`, `market_id: u32 (topic)`, `amount: i128` | A user attempts to withdraw with zero collateral deposited (edge case for monitoring) |
| `market_closed_to_deposits` | `version: u32 (topic)`, `market_id: u32 (topic)`, `admin: Address`, `closed_at: u64` | An admin closes a market to new collateral deposits |
| `market_resolved` | `version: u32 (topic)`, `market_id: u32 (topic)`, `oracle_pubkey: BytesN<32>`, `resolver: Address`, `outcome: bool`, `resolved_at: u64` | A market is resolved with an oracle-signed outcome |
| `market_canceled` | `version: u32 (topic)`, `market_id: u32 (topic)`, `canceler: Address`, `canceled_at: u64` | An admin cancels a market before resolution |
| `market_voided` | `version: u32 (topic)`, `market_id: u32 (topic)`, `voided_by: Address`, `voided_at: u64` | The registered resolution contract voids a market via `void_market` (Issue #708) — dispute outcome where neither side could be vindicated on-chain; `voided_by` is the resolution contract address. Distinct from `market_canceled` (admin `cancel_market`) though both land in `Canceled`. |
| `position_limit_exceeded` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `side_yes: bool` | A trade/position change is rejected because a share balance would go negative |
| `position_updated` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `yes_shares: i128`, `no_shares: i128`, `locked_collateral: i128` | A user's position (share balances / locked collateral) changes |
| `trade_executed` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `quantity: i128`, `price_bps: i128`, `side_yes: bool`, `executed_at: u64` | A user executes a trade (buy or sell of YES/NO shares) |
| `validation_failed` | `version: u32 (topic)`, `context: Symbol (topic)`, `error_code: u32` | A validation step fails, recording the call-site context and `ContractError` code |
| `position_settled` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `payout: i128`, `settled_at: u64` | A user's position is settled and payout transferred after resolution |
| `positions_batch_settled` | `version: u32 (topic)`, `market_id: u32 (topic)`, `users: Vec<Address>`, `payouts: Vec<i128>`, `settled_at: u64` | One aggregated event summarizing a `batch_settle_positions` call (replaces per-user `position_settled`/`position_updated` pairs) |
| `oracle_signature_verified` | `version: u32 (topic)`, `market_id: u32 (topic)`, `outcome: bool`, `verified_at: u64` | An oracle's Ed25519 signature is successfully verified during resolution |
| `fee_calculated` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `fee_amount: i128`, `available_after_fee: i128` | A withdrawal fee is calculated for a user |
| `admin_transfer_proposed` | `version: u32 (topic)`, `current_admin: Address (topic)`, `pending_admin: Address (topic)`, `proposed_at: u64` | The current admin proposes transferring the admin role |
| `admin_transfer_accepted` | `version: u32 (topic)`, `old_admin: Address (topic)`, `new_admin: Address (topic)`, `accepted_at: u64` | The pending admin accepts the proposed transfer |
| `market_oracle_updated` | `market_id: u32 (topic)`, `admin: Address`, `old_oracle_pubkey: BytesN<32>`, `new_oracle_pubkey: BytesN<32>`, `updated_at: u64` | The admin rotates a market's oracle public key |
| `treasury_set` | `version: u32 (topic)`, `treasury: Address (topic)`, `set_at: u64` | The Treasury contract address is registered on the market |
| `fee_rate_change_proposed` | `version: u32 (topic)`, `new_rate_bps: i128`, `effective_at: u64` | An admin proposes a new withdrawal fee rate, effective at a future timestamp |
| `fee_rate_change_executed` | `version: u32 (topic)`, `new_rate_bps: i128`, `executed_at: u64` | A previously-proposed fee rate change takes effect |
| `admin_renounce_proposed` | `version: u32 (topic)`, `admin: Address (topic)`, `proposed_at: u64` | The admin proposes renouncing the admin role |
| `admin_renounced` | `version: u32 (topic)`, `former_admin: Address (topic)`, `renounced_at: u64` | The admin renounce is finalized (admin role given up) |
| `oracle_adapter_configured` | `adapter_type: AdapterType (topic)`, `enabled: bool`, `configured_at: u64` | The admin enables/disables the Reflector or Pyth oracle adapter |
| `fee_waiver_added` | `account: Address (topic)`, `admin: Address`, `added_at: u64` | The admin adds an address to the withdrawal fee waiver list |
| `fee_waiver_removed` | `account: Address (topic)`, `admin: Address`, `removed_at: u64` | The admin removes an address from the fee waiver list |
| `position_token_mismatch_detected` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `yes_shares: i128`, `no_shares: i128`, `yes_token_balance: i128`, `no_token_balance: i128` | The reconciliation guard finds a user's `Position` shares and `OutcomeToken` balances have diverged (raised before `update_position`/`settle_position` reject) |
| `position_tokens_reconciled` | `version: u32 (topic)`, `market_id: u32 (topic)`, `user: Address (topic)`, `admin: Address`, `yes_delta_applied: i128`, `no_delta_applied: i128`, `reconciled_at: u64` | An admin repairs a Position/OutcomeToken divergence via `reconcile_position_tokens` (deltas are the signed mint/burn applied to the OutcomeToken balance) |

## Treasury (`contracts/treasury/src/events.rs`)

| Event/Topic | Fields (name: type) | Emitted When |
|---|---|---|
| `treasury_initialized` | `admin: Address (topic)`, `market_contract: Address (topic)`, `initialized_at: u64` | The Treasury contract is initialized with an admin and linked market contract |
| `fee_collected` | `market_id: u32 (topic)`, `token: Address (topic)`, `fee_amount: i128`, `new_token_balance: i128`, `new_cumulative_fees: i128` | A fee is collected from a market's withdrawal event |
| `fees_withdrawn` | `token: Address (topic)`, `to: Address (topic)`, `amount: i128`, `remaining_token_balance: i128` | The admin withdraws collected fees to a destination address |
| `admin_transferred` | `old_admin: Address (topic)`, `new_admin: Address (topic)`, `transferred_at: u64` | The Treasury admin role is transferred |
| `market_contract_updated` | `old_market_contract: Address (topic)`, `new_market_contract: Address (topic)` | The Treasury's registered market contract address is rotated |
| `market_added` | `market_contract: Address (topic)` | A market contract is added to the Treasury's multi-market registry |
| `market_removed` | `market_contract: Address (topic)` | A market contract is removed from the Treasury's registry |
| `stakeholders_updated` | `stakeholder_count: u32`, `updated_at: u64` | The fee-distribution stakeholder list is updated |
| `fees_distributed` | `token: Address (topic)`, `distributed_amount: i128`, `remaining_token_balance: i128`, `stakeholder_count: u32`, `distributed_at: u64` | Fees for `token` are distributed across all configured stakeholders (once per `distribute_fees` call) |
| `treasury_paused` | `admin: Address (topic)`, `paused_at: u64` | The Treasury is paused for emergency maintenance |
| `treasury_unpaused` | `admin: Address (topic)`, `unpaused_at: u64` | The Treasury is unpaused |

## Resolution (`contracts/resolution/src/events.rs`)

| Event/Topic | Fields (name: type) | Emitted When |
|---|---|---|
| `resolution_registered` | `factory: Address (topic)`, `market_contract: Address`, `registered_at: u64` | The Resolution contract is initialized, registering the factory/market relationship |
| `candidate_proposed` | `candidate_id: u32 (topic)`, `market_id: u32 (topic)`, `outcome: bool`, `proposer: Address`, `evidence_uri: String`, `challenge_deadline: u64`, `signature_expiry: u64` | A proposer submits a signed resolution candidate — via either `propose` (V1) or `propose_v2` (#701; same event, `signature_expiry` doubles as V2's `valid_until`) |
| `candidate_challenged` | `candidate_id: u32 (topic)`, `market_id: u32 (topic)`, `challenger: Address`, `challenge_uri: String`, `challenged_at: u64` | A challenger disputes a candidate before its challenge deadline |
| `candidate_finalized` | `candidate_id: u32 (topic)`, `market_id: u32 (topic)`, `outcome: bool`, `finalized_at: u64` | A candidate is finalized after its challenge window closes (`finalize`) — invokes `resolve_market` or `resolve_market_v2` on the market contract depending on how the candidate was proposed (#701) |
| `candidate_appealed` | `candidate_id: u32 (topic)`, `market_id: u32 (topic)`, `outcome: bool`, `proposer: Address`, `appeal_round: u32`, `evidence_uri: String`, `challenge_deadline: u64`, `appealed_at: u64` | A challenged candidate is re-proposed/appealed for another round. V1-only (`appeal` rejects a V2-proposed candidate, #701) |
| `emergency_mode_changed` | `new_mode: EmergencyMode (topic)`, `admin: Address`, `changed_at: u64` | Admin changes the mirrored emergency mode (`set_emergency_mode`), coordinated with the Market and Treasury contracts (#662, wired up for resolution by #701) |
| `market_voided` | `candidate_id: u32 (topic)`, `market_id: u32 (topic)`, `voided_at: u64` | Resolution `void_market` runs: the proposer's bond is split/slashed, challengers are refunded, and the market contract's `void_market` is invoked to move the market to `Canceled` (Issue #708). The market contract emits its own `market_voided` for the status transition. |

## Outcome Token (`contracts/outcome-token/src/events.rs`)

| Event/Topic | Fields (name: type) | Emitted When |
|---|---|---|
| `token_minted` | `market_id: u32 (topic)`, `user: Address (topic)`, `kind: TokenKind`, `amount: i128`, `new_balance: i128` | Outcome tokens (YES/NO) are minted to a user |
| `token_burned` | `market_id: u32 (topic)`, `user: Address (topic)`, `kind: TokenKind`, `amount: i128`, `new_balance: i128` | Outcome tokens are burned from a user |
| `token_transferred` | `market_id: u32 (topic)`, `from: Address (topic)`, `to: Address`, `kind: TokenKind`, `amount: i128` | Outcome tokens are transferred between two users |

## Notes for indexers

- `market_id`/`candidate_id`/topic addresses are indexed (declared with
  `#[topic]`) specifically so off-chain services can filter the Soroban
  event stream server-side rather than scanning every event.
- `TokenKind` (outcome-token) and `AdapterType` (market) are contract-defined
  enums — see `contracts/outcome-token/src/types.rs` and
  `contracts/market/src/types.rs` respectively for their variants.
- Some Market events (e.g. `market_oracle_updated`, `oracle_adapter_configured`,
  `fee_waiver_added`/`fee_waiver_removed`) do **not** carry the `version`
  topic that most other Market events do — this reflects when each event was
  added relative to the Issue #500 schema-versioning change, not an error in
  this table.
