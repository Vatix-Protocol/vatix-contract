# Staging (Testnet) Dry-Run Checklist

Run this checklist against **testnet** before any mainnet upgrade of the
Market, Treasury, Resolution, or Outcome Token contracts. It exercises the
same ordering and version-skew guarantees as
[`UPGRADE_PLAYBOOK.md`](UPGRADE_PLAYBOOK.md), but as a manual sign-off list
you can attach to a PR or upgrade ticket.

Each item that has a corresponding script call is annotated so you can
follow along instead of hand-running individual `stellar` commands.

## 1. Pre-flight (scripted)

- [ ] `bash scripts/upgrade/check-upgrade.sh` exits `0`.
  - [ ] Phase A: no storage-version drift reported for market/treasury.
  - [ ] Phase B: any pinned hash in `expected-hashes.json` matches the fresh
        build (unpinned entries are fine — just note in the ticket that
        they're intentionally unpinned for this rollout).
  - [ ] Phase C: `UpgradeRequired` regression tests pass for market and
        treasury.
- [ ] `contracts/market/STORAGE_MIGRATION_GUIDE.md` "Version History" has an
      entry for the version being shipped, if market's `STORAGE_VERSION`
      changed.
- [ ] `scripts/upgrade/version-matrix.json` has been updated in the same PR
      as any `STORAGE_VERSION` bump (market or treasury).

## 2. Deploy to testnet (in order — see `UPGRADE_PLAYBOOK.md#deploy-order`)

- [ ] Market deployed: `bash scripts/deploy-testnet.sh` (or
      `CONTRACT_DIR=contracts/market`).
- [ ] Treasury deployed: `CONTRACT_DIR=contracts/treasury bash scripts/deploy-testnet.sh`.
- [ ] Outcome Token deployed: `CONTRACT_DIR=contracts/outcome-token bash scripts/deploy-testnet.sh`.
- [ ] Resolution deployed: `CONTRACT_DIR=contracts/resolution bash scripts/deploy-testnet.sh`
      (pass the Market contract ID from the first step to `initialize`).
- [ ] All four new contract IDs recorded in `deployments/testnet.json`
      (see `deployments/README.md`) — **do not overwrite the old IDs**,
      append/replace only after the checklist below passes so
      `rollback.sh` has something to recover from.

## 3. Wire the four contracts together

- [ ] `MarketContract::set_treasury` + `set_fee_rate`
- [ ] `MarketContract::set_outcome_token_contract`
- [ ] `MarketContract::set_resolution_contract`
- [ ] `OutcomeTokenContract::set_market_contract`
- [ ] `TreasuryContract::initialize` (or `set_market_contract`) with the new
      Market address
- [ ] Cross-check against
      [`docs/cross-contract-call-graph.md`](../../docs/cross-contract-call-graph.md) —
      every edge in the call graph that your upgrade touches has both sides
      re-wired, not just one.

## 4. Verify version skew is detected (before enabling traffic)

- [ ] `bash scripts/testnet-smoke.sh` succeeds against the **new** Market
      deployment.
- [ ] Confirm the **old** deployment now rejects state-mutating calls with
      `UpgradeRequired` (or the treasury equivalent) if its storage version
      no longer matches its own compiled code — i.e. you did not
      accidentally reuse a contract ID across versions.
- [ ] Re-run `bash scripts/upgrade/check-upgrade.sh` one more time after
      wiring — this is the "get pass/fail before enabling traffic" gate
      called out in the issue's acceptance criteria.

## 5. Smoke test critical flows on the new deployment

- [ ] Create a market.
- [ ] Deposit collateral.
- [ ] Trade (buy/sell shares) and confirm Outcome Token mint/burn fires.
- [ ] Trigger a withdrawal that crosses the fee path and confirm
      `Treasury::collect_fee` is invoked.
- [ ] Run a full Resolution lifecycle: `propose` → (optional `challenge`) →
      `finalize` → confirm it calls back into `Market::resolve_market`.

## 6. Rollback rehearsal (dry run only — do not apply during a healthy rollout)

- [ ] `bash scripts/upgrade/rollback.sh HEAD` runs cleanly and shows the
      diff you'd expect if this upgrade needed to be undone.
- [ ] The team knows where the "Rollback" section of `UPGRADE_PLAYBOOK.md`
      lives before starting the real upgrade, not after something breaks.

## 7. Sign-off

- [ ] All boxes above checked.
- [ ] Ticket/PR links this checklist and the `check-upgrade.sh` output.
- [ ] Only after this checklist passes on testnet does the same sequence
      get repeated against mainnet, per
      `contracts/market/STORAGE_MIGRATION_GUIDE.md` → "For Mainnet
      Deployments".
