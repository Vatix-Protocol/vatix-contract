# Admin Rotation Ceremony

Runbook for rotating the admin key of the Vatix contracts (#913). The
entrypoints and their auth checks are inventoried in
[`AUTH_TABLE.md`](../AUTH_TABLE.md); this document is the operational
procedure around them. Security disclosures go through
[`SECURITY.md`](../SECURITY.md).

## Invariants

1. **Two-party or time-delayed only.** No production rotation is a single
   instant call. Market uses a two-step propose/accept; Treasury uses a 48h
   timelock (`ADDRESS_TIMELOCK_SECONDS = 172_800`).
2. **Both keys prove possession.** Every mutating step calls `require_auth()`
   on the acting key *and* checks it against stored state (`AUTH_TABLE.md`,
   "two-step pattern"). Nothing can be completed by merely naming an address.
3. **New admin is an account, never a contract.** Market's `propose_admin`
   rejects contract addresses with `InvalidAdmin` (`validate_admin_address`).
4. **Cancelable until completion.** The current admin can abort at any point
   before the final step (`cancel_admin_transfer` / `cancel_admin`).
5. **Fail-closed.** Any wrong key, missing pending proposal, or early execute
   returns an error and leaves the stored admin unchanged.

## Scope per contract

| Contract | Rotation path | Notes |
|----------|---------------|-------|
| Market | `propose_admin(current_admin, new_admin)` → `accept_admin(new_admin)`; abort with `cancel_admin_transfer(current_admin)` | Emits `admin_transfer_proposed` / `admin_transfer_accepted`. |
| Treasury | `propose_admin(caller, new_admin)` → wait 48h → `execute_admin()`; abort with `cancel_admin(caller)` | `execute_admin` is callable by anyone once due; the timelock is the access control. `transfer_admin` is an immediate path reserved for emergency rotation only (see below). |
| Resolution | none | Admin is fixed at `initialize`. Rotation requires a WASM upgrade/redeploy via [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](../scripts/upgrade/UPGRADE_PLAYBOOK.md). |
| Outcome-token | none | Admin is fixed at `initialize`; same as Resolution. |

## Ceremony

### 1. Prepare (T-1 day)

- Generate the new admin key on an offline / hardware signer. Never paste a
  secret key into a shell history, CI variable, issue, or log.
- Record only the new **public** address (`G...`) in the change ticket.
- Confirm the target network and contract IDs against
  `deployments/testnet.json` (or the mainnet registry) — rehearse on testnet
  first with identical steps.
- Announce the window to operators; pause no user flows unless the rotation
  is an emergency (see below).

### 2. Propose (current admin signs)

```sh
stellar contract invoke --id $MARKET_ID --source $CURRENT_ADMIN -- \
  propose_admin --current_admin $CURRENT_ADMIN_ADDR --new_admin $NEW_ADMIN_ADDR

stellar contract invoke --id $TREASURY_ID --source $CURRENT_ADMIN -- \
  propose_admin --caller $CURRENT_ADMIN_ADDR --new_admin $NEW_ADMIN_ADDR
```

Verify: the Market `admin_transfer_proposed` event names the expected
addresses, and Treasury's pending admin + `effective_at` match.

### 3. Complete

- **Market:** the new admin signs `accept_admin --new_admin $NEW_ADMIN_ADDR`.
  This proves the new key is live *before* the old one loses control.
- **Treasury:** after `effective_at`, anyone submits `execute_admin`.

### 4. Verify

- Treasury's `admin` getter returns `$NEW_ADMIN_ADDR`; Market (no admin
  getter) emitted `admin_transfer_accepted` naming `$NEW_ADMIN_ADDR`.
- A harmless admin call (e.g. a no-op `pause`/`unpause` on testnet, or a
  read of an admin-gated config) succeeds with the new key and fails with
  the old key (`NotAdmin` / `Unauthorized`).
- Indexers/alerts observed the event; an `admin_transfer_canceled` or an
  unexpected `admin_transfer_proposed` during the window is a page-worthy alert.

### 5. Decommission

- Revoke/destroy the old key material and record the date in the ticket.
- Update any runbooks or monitoring allowlists that pinned the old address.

## Rollback

- **Before completion:** current admin calls `cancel_admin_transfer`
  (Market) / `cancel_admin` (Treasury). State is unchanged.
- **After completion:** there is no undo; run the ceremony again from the new
  admin back to the intended key.

## Emergency rotation (suspected key compromise)

1. Call `set_emergency_mode` / `pause` on Market, Treasury and Resolution to
   freeze money paths while the key is rotated.
2. Market: `propose_admin` + `accept_admin` immediately (no timelock).
3. Treasury: use `transfer_admin` (immediate) rather than waiting 48h, and
   `cancel_admin` any pending proposal the attacker may have queued.
4. Audit events since the suspected compromise for `*_proposed` timelock
   entries (oracle, treasury, outcome-token, resolution contract, fee rate,
   threshold signers) and `cancel_*` every one not initiated by operators.
5. Unpause only after step 4 is complete; file a report per `SECURITY.md`.
