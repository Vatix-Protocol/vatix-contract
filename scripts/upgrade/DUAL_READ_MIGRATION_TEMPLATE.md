# Dual-Read Migration Template (v(N) → v(N+1))

`contracts/market/STORAGE_MIGRATION_GUIDE.md` currently documents a
**fresh-deployment-only** migration model: every `STORAGE_VERSION` bump so
far has shipped as "redeploy, no data migration available." That's the
right call when the old data genuinely can't be reinterpreted. But it forces
a hard cutover with a maintenance window every time.

This template is the alternative for the *next* bump where the new field is
optional/derivable and you'd rather let a single live deployment serve reads
of both the old and new stored shape during a compatibility window, then
drop the old-shape branch once every reader/writer has moved to the new
version. It is **not applied to any contract in this PR** — it's a
copy-paste starting point for whoever does the next real bump, using
market's actual `Market` type and `get_market`/`set_market` accessors
(`contracts/market/src/storage.rs`) as the concrete example.

## Invariants

Any dual-read bump MUST preserve all of the following. If a change would
violate one, it is not a dual-read bump — use the fresh-deployment path in
`STORAGE_MIGRATION_GUIDE.md` instead.

1. **New shape is canonical.** Business logic, events, and tests only ever
   see the new struct. The old struct is referenced *only* by the storage
   accessor's fallback branch.
2. **Writes are monotonic.** Every write persists the new shape. There is
   no downgrade path and no code path that writes the old shape.
3. **Reads are total.** For any record written under either shape,
   `get_market` returns the same logical `Market` value (old records are
   upcast with the new field's documented default).
4. **Version gate is unchanged.** `STORAGE_VERSION` still bumps and
   `assert_version` still gates every accessor. Dual-read only changes how
   a *single* `StorageKey::Market(id)` entry is deserialized, not whether
   the contract as a whole is considered upgraded.
5. **Fail-closed on ambiguity.** If a record decodes as neither the new nor
   the old shape, the accessor returns a stable error rather than a
   defaulted/zeroed `Market`.
6. **Idempotent under replay.** Reading and re-persisting a record is
   idempotent: `set_market(get_market(id))` is a no-op on the logical value,
   so concurrent or replayed writes cannot corrupt state.

## The pattern

1. Keep the **old** `#[contracttype]` struct definition around, renamed with
   a version suffix, instead of deleting it.
2. The **new** struct is what the rest of the contract uses everywhere
   (business logic, events, tests) — it never sees the old shape directly.
3. The storage accessor tries the new key/shape first; if that's absent, it
   falls back to reading the old shape and **upcasts** it into the new
   struct using a sensible default for the added field.
4. Every *write* always writes the new shape — there is no "downgrade"
   path. Once a record is read once, it's re-persisted in the new shape on
   the next write (lazy migration), so live traffic gradually rewrites old
   records without a bulk migration job.
5. `STORAGE_VERSION` still bumps and `assert_version` still gates every
   accessor exactly as today — dual-read only changes how a *single*
   `StorageKey::Market(id)` entry is deserialized, not whether the contract
   as a whole is considered upgraded. This gives you a compatibility window
   for *data shape*, while the coarse-grained `assert_version` check still
   protects against a genuinely incompatible contract binary being pointed
   at storage it doesn't understand.

## Worked example: adding `Market::settlement_note: Option<String>` in a hypothetical v5

```rust
// contracts/market/src/storage.rs

/// Shape of `Market` as stored under STORAGE_VERSION 4. Kept only so
/// `get_market` can dual-read pre-migration records — do not add new
/// fields here or reference it from business logic.
#[contracttype]
pub struct MarketV4 {
    pub id: u32,
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub status: MarketStatus,
    pub result: Option<bool>,
    pub creator: Address,
    pub created_at: u64,
    pub collateral_token: Address,
    pub price_bps: i128,
    pub resolver: Option<Address>,
    pub resolved_at: Option<u64>,
    pub adapter_type: AdapterType,
    pub outcome_count: u32,
    // ...every other field `Market` had at v4, unchanged...
}

impl From<MarketV4> for Market {
    fn from(old: MarketV4) -> Self {
        Market {
            id: old.id,
            question: old.question,
            end_time: old.end_time,
            oracle_pubkey: old.oracle_pubkey,
            status: old.status,
            result: old.result,
            creator: old.creator,
            created_at: old.created_at,
            collateral_token: old.collateral_token,
            price_bps: old.price_bps,
            resolver: old.resolver,
            resolved_at: old.resolved_at,
            adapter_type: old.adapter_type,
            outcome_count: old.outcome_count,
            settlement_note: None, // new field's default for pre-v5 records
        }
    }
}

pub fn get_market(env: &Env, market_id: u32) -> Result<Option<Market>, ContractError> {
    assert_version(env)?;
    let key = StorageKey::Market(market_id);

    if let Some(market) = env.storage().persistent().get::<_, Market>(&key) {
        return Ok(Some(market));
    }
    // Dual-read fallback: try decoding the pre-v5 shape and upcast it.
    // Safe because Soroban storage is untyped bytes at rest — a `MarketV4`
    // record simply fails to deserialize as `Market` (extra field) and
    // vice versa, so trying both is the mechanism, not a type-unsafe cast.
    if let Some(old) = env.storage().persistent().get::<_, MarketV4>(&key) {
        return Ok(Some(old.into()));
    }
    Ok(None)
}

pub fn set_market(env: &Env, market_id: u32, market: &Market) -> Result<(), ContractError> {
    assert_version(env)?;
    crate::validation::validate_outcome_count(market.outcome_count)?;
    // Always writes the *new* shape — this is the lazy-migration step.
    env.storage()
        .persistent()
        .set(&StorageKey::Market(market_id), market);
    Ok(())
}
```

## Authz expectations

Dual-read changes the *storage shape*, never the *authorization surface*.

- `get_market` stays a read-only, unauthenticated accessor — it must not
  gain any privileged behavior as part of a dual-read bump.
- `set_market` remains callable only from the same internal, already-authorized
  call sites as before. Do not expose a new public entrypoint that lets an
  untrusted client write a `Market` (or a `MarketV(N)`) directly.
- Any new privileged surface introduced alongside the bump is
  **deny-by-default**: it must be gated behind an explicit admin/role check
  and, if it touches the money path, behind a feature flag or kill-switch
  (see below).
- Stable error codes: reuse the existing `ContractError` variants
  (`UpgradeRequired` from `assert_version`, validation errors from
  `validate_outcome_count`). Do not introduce ad-hoc panics for the
  dual-read branch — a record that decodes as neither shape should surface a
  stable, documented error code, not a raw unwrap.

## Fail-closed behavior

- **Version mismatch:** `assert_version` fails closed exactly as today — a
  binary pointed at storage it doesn't understand returns `UpgradeRequired`
  rather than guessing.
- **Undecodable record:** if a `StorageKey::Market(id)` entry decodes as
  neither `Market` nor `MarketV(N)`, return a stable error. Never fall back
  to a defaulted/zeroed `Market`, which could silently misprice or mis-settle.
- **Dependency outage:** the contract is the source of truth for balances,
  swaps, and admin. If an off-chain dependency (RPC/DB/Redis) used by
  tooling is unavailable, writes fail closed — no partial or best-effort
  migration writes.
- **Idempotency / replay:** the lazy-migration write is idempotent. A
  replayed or concurrent `set_market` with the same logical value leaves
  state unchanged, so retries and duplicate submissions are safe.

## Rollback / flag strategy

- Land the bump behind a feature flag or kill-switch when it touches the
  money path or mainnet. The flag gates the *new* behavior; the old-shape
  read branch is additive and safe to keep enabled.
- Document the rollback in the PR description: reverting the binary restores
  the previous `STORAGE_VERSION` gate, and because writes are monotonic
  (new shape only), a rollback must be paired with the fresh-deployment
  path if the old binary cannot read the new shape.
- Set an explicit end date for the compatibility window and remove the
  `MarketV(N)` struct + `From` impl in a follow-up PR once every record has
  been lazily migrated or explicitly backfilled.

## Rules of thumb

- **Only do this when the new field has a safe default for old records.**
  If the new field can't be derived or defaulted (e.g. it changes the
  meaning of an *existing* field, like the v1→v2 `locked_collateral` fix
  documented in `STORAGE_MIGRATION_GUIDE.md`), dual-read isn't
  appropriate — that needs the "fresh deployment + explicit recompute"
  path instead.
- **Set an end date for the compatibility window.** Once you're confident
  every record has been touched (lazily migrated) or you've run an
  explicit backfill, delete the `MarketV(N)` struct and its `From` impl in
  a follow-up PR — don't let dual-read branches accumulate indefinitely.
- **Test both branches.** Add a unit test that writes a `MarketV(N)` record
  directly (bypassing `set_market`), then asserts `get_market` upcasts it
  correctly — mirroring the existing `test_wrong_version_returns_upgrade_required`
  style already used in `contracts/market/src/storage.rs`. Add a negative
  test for the undecodable-record fail-closed path.
- **Still update `STORAGE_MIGRATION_GUIDE.md` and `scripts/upgrade/version-matrix.json`**
  exactly as any other version bump — dual-read changes *how* the bump is
  rolled out, not whether it needs to be documented. Keep the guide's
  cross-links to this template consistent; remove any copy that still
  claims a bump is "fresh-deployment-only" once a dual-read bump ships.
