# Contributing to Vatix Protocol

Thanks for contributing! This guide covers the local tooling and workflow for the
Vatix-Protocol monorepo. For security-sensitive changes, also read
[`SECURITY.md`](./SECURITY.md).

## Prerequisites

- **Node.js** >= 18
- **pnpm** >= 8 (the workspace is managed with pnpm; do not use npm/yarn)
- **Rust** toolchain (for `contracts/*` crates)

Install pnpm if you do not have it:

```sh
corepack enable
corepack prepare pnpm@latest --activate
```

## Workspace layout

The monorepo is a pnpm workspace. Packages are declared in
[`pnpm-workspace.yaml`](./pnpm-workspace.yaml):

- `apps/*` — frontend / service applications
- `contracts/*` — Soroban contract crates and their tooling

## Tooling scripts

All workspace operations are driven from the root [`package.json`](./package.json)
and delegate to every workspace package via `pnpm -r` / `--filter`. Scripts are
**fail-closed**: any package failure exits non-zero and aborts the run. Do not add
`|| true` or otherwise swallow errors.

| Script | Command | Purpose |
| --- | --- | --- |
| `pnpm install` | `pnpm install` | Install all workspace dependencies |
| `pnpm build` | `pnpm -r run build` | Build every workspace package |
| `pnpm test` | `pnpm -r run test` | Run every workspace package's tests |
| `pnpm lint` | `pnpm -r run lint` | Lint every workspace package |
| `pnpm clean` | `pnpm -r run clean` | Remove build artifacts across the workspace |

Run a single package with a filter, e.g.:

```sh
pnpm --filter <package-name> run test
```

## Workflow

1. Fork and branch from `main`.
2. Make your change with focused commits.
3. Run `pnpm install`, then `pnpm lint` and `pnpm test` before opening a PR.
4. Open a PR describing the change, its invariants, and any rollback/flag strategy
   for money-path or mainnet-affecting work.

## Security

- Never commit secrets or credentials.
- The server/contract remains the source of truth for balances, swaps, and admin.
- Authorize and rate-limit every external entrypoint; deny-by-default for new
  privileged surfaces.
- Report vulnerabilities per [`SECURITY.md`](./SECURITY.md).

### Prerequisites

- `stellar` CLI (Soroban-enabled) on `PATH`.
- `wasm32-unknown-unknown` target installed (`rustup target add wasm32-unknown-unknown`).
- A local Soroban network running (`stellar network start local` or the
  `soroban-testnet`/`quickstart` container).

### 1. Build

```bash
cd contracts/market
cargo build --target wasm32-unknown-unknown --release
```

The artifact lands at
`target/wasm32-unknown-unknown/release/vatix_market_contract.wasm`.

### 2. Deploy

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/vatix_market_contract.wasm \
  --source <admin-identity> \
  --network local
```

Record the returned contract id — every later step needs it.

### 3. Initialize

`initialize` is a **privileged entrypoint**: it sets the admin and the
collateral token, and must be callable exactly once. The deploy path must
invoke it from the admin identity only; a second call (or a call from any
other identity) must fail closed with the contract's existing
`AlreadyInitialized` / auth error rather than silently re-configuring state.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <admin-identity> \
  --network local \
  -- initialize \
  --admin <admin-address> \
  --collateral_token <token-address>
```

### 4. Smoke-verify

```bash
# Read back the admin — must equal the address passed to initialize.
stellar contract invoke --id <contract-id> --network local -- get_admin

# Negative check: a non-admin identity must be rejected on a privileged call.
stellar contract invoke --id <contract-id> --source <non-admin-identity> \
  --network local -- set_paused --paused true   # expect auth failure
```

### Invariants the localnet deploy must preserve

- **Server/contract is the source of truth** for balances, swaps, and admin
  state. The deploy path never writes balances or admin state out-of-band;
  it only calls the contract's own entrypoints.
- **Deny-by-default for privileged surfaces.** `initialize`, admin setters,
  and pause/kill-switch calls are authorized against the stored admin. A
  localnet deploy must not weaken this — no "skip auth on local" branches.
- **Fail closed on dependency outage.** If the RPC/network is unreachable,
  writes (deploy, initialize, admin calls) must abort rather than proceed
  against stale state. Reads may retry; writes must not.
- **No secrets in repo or logs.** Use named `stellar` identities backed by
  the local keystore; never commit secret keys, mnemonics, or `.env` files
  containing them, and never echo them in deploy output.
- **Idempotency.** Re-running the deploy path against an already-initialized
  contract must fail closed (not re-initialize). Re-running a read-only
  smoke check is safe.

### Testnet vs mainnet / address drift

Localnet contract ids and token addresses differ from testnet and mainnet.
Never copy a localnet id into a testnet/mainnet runbook or script. Any
money-path or mainnet-affecting change must land behind a feature flag or
kill-switch, with the rollback documented in the PR description.

See [SECURITY.md § Localnet and privileged surfaces](SECURITY.md#localnet-and-privileged-surfaces)
for the security rationale behind these invariants.

## Issue scripts quality bar (`scripts/issues/`)

Scripts under `scripts/issues/` are operational tooling that can touch
money-path state. They must meet the bar below before merge. See
[`scripts/issues/README.md`](scripts/issues/README.md) for the full reference.

### Invariants

- **Idempotency.** Replayed or concurrent runs must be safe. Every write
  entrypoint takes a stable idempotency key and must not double-apply effects.
- **Fail-closed writes.** On RPC/DB/Redis outage, writes abort with a typed
  error rather than partially applying. Reads may degrade; writes must not.
- **Deny-by-default authz.** Privileged surfaces require an explicit role and
  reject untrusted callers. New privileged entrypoints start denied.
- **No secrets.** Never log or commit secrets, keys, or tokens. Redact
  sensitive fields in logs and metrics.

### Error codes and correlation ids

- Use stable, documented error codes for script entrypoints; do not reuse a
  code for a different failure mode.
- Propagate a correlation id through every entrypoint and include it in logs
  and error responses so operators can trace a run end to end.

### Observability

- Emit ops-safe metrics and logs on money paths (counts, latencies, outcomes)
  without leaking secrets or user-identifying data.
- Metrics must be actionable: alert on write failures and authz denials.

### Safety

- Feature-flag or kill-switch any money-path or mainnet-affecting change.
- Document the rollback strategy in the PR description.
- Respect testnet vs mainnet address drift; never hardcode mainnet addresses
  in scripts without an explicit, reviewed flag.

## Pull request checklist

- [ ] Behavior matches the cited docs for the issue.
- [ ] Authz, idempotency, and fail-closed behavior covered by tests.
- [ ] [AUTH_TABLE.md](AUTH_TABLE.md) updated for any added, removed, renamed,
      or re-gated entrypoint; `bash scripts/check-auth-table.sh` passes (#892).
- [ ] Docs/runbooks updated; mainnet safety respected.
- [ ] Observability is actionable; metrics on money paths.
- [ ] Rollback/flag strategy documented in the PR description.
