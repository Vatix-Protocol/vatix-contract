# Contributing to Vatix Protocol

Thanks for contributing to the Vatix-Protocol monorepo. This guide covers the
baseline expectations for every package, the exact build/test commands that
match the repo layout and CI, and a dedicated section for the
`vatix-contract` issue scripts.

## Getting started

1. Fork the repository and create a topic branch off `main`.
2. Keep changes scoped to a single issue; avoid unrelated refactors.
3. Enable the repo's Git hooks (see [Git hooks](#git-hooks)) so the fast local
   gate runs before every commit.
4. Run the package's build and test commands locally (see
   [Build and test](#build-and-test)) before opening a PR.
5. Open a PR that links the issue it resolves and describes the rollback plan
   for any money-path or mainnet-affecting change.

## General expectations

- Match existing patterns, types, and module structure.
- Never commit secrets, tokens, or credentials.
- Keep CI green; add a required check if a new surface is ungated.
- Update docs and runbooks when behavior changes.

## Git hooks

The repo ships versioned Git hooks under [`.githooks/`](.githooks/). They are a
**fast local gate** that catches common mistakes before a commit leaves your
machine — they are not a replacement for CI (see
[Relationship to CI](#relationship-to-ci)).

### What hooks exist

- **`.githooks/pre-commit`** — runs on `git commit`, before the commit is
  created. It is the local pre-flight check for the staged change set (for
example formatting/lint or fast checks the repo wires up). If it exits
  non-zero, the commit is aborted so you can fix the issue and re-stage.

If you add or change a hook, document it here in the same PR so contributors
know what runs and when.

### When each hook runs

| Hook | Trigger | Effect on failure |
| --- | --- | --- |
| `.githooks/pre-commit` | `git commit` (before the commit object is written) | Aborts the commit; nothing is committed |

### Install / enable the hooks locally

Git does not use `.githooks/` by default. Point Git at it once per clone:

```bash
git config core.hooksPath .githooks
```

Make sure the hook scripts are executable (they are committed with the
executable bit; if your checkout lost it, restore it):

```bash
chmod +x .githooks/*
```

### Verify the hooks are active

```bash
# Should print: .githooks
git config --get core.hooksPath

# Should list the hook scripts
ls -l .githooks
```

If `core.hooksPath` is unset, the hooks are **not** running and your commits
will skip the local gate.

### Relationship to CI

Hooks and CI are complementary, not interchangeable:

- **Hooks** run locally, only on your machine, and only for the hooks you have
  enabled. They are fast and give immediate feedback, but they can be skipped
  (`--no-verify`) and are not enforced for anyone else.
- **CI** (`.github/workflows/ci.yml`) runs on every push/PR on a clean
  checkout and is the **source of truth** for merge gating. It cannot be
  bypassed locally.

A green local hook run is a convenience, not a guarantee. Always rely on CI
for the authoritative build/test result, and never use `--no-verify` to push a
change you have not otherwise validated.

## Reporting a security vulnerability

Do **not** open a public issue, PR, or discussion for a suspected
vulnerability. Follow the private disclosure process in
[SECURITY.md](SECURITY.md) — it lists the supported versions, the in-scope
surfaces, and the private reporting channels. If you are unsure whether
something is a vulnerability, report it privately anyway; the maintainers
will triage it.

## Toolchain and prerequisites

Install these before building or testing so you can reproduce CI locally.

- **Rust** — stable toolchain via [rustup](https://rustup.rs/).
- **WASM target** — `rustup target add wasm32-unknown-unknown` (required to
  build the `market` contract for Soroban).
- **Node.js** — LTS (see `.nvmrc` / `engines` in `apps/web/package.json` if
  present).
- **Package manager** — use the lockfile committed in `apps/web` (npm, pnpm,
  or yarn) so installs match CI.
- **`stellar` CLI** (Soroban-enabled) on `PATH` — only needed for the
  localnet deploy path below.

## Build and test

Commands below mirror `.github/workflows/ci.yml`. Run them from the repo root
unless a `cd` is shown.

### Contract package (`contracts/market`)

The contract is a Cargo workspace member under `contracts/market`.

```bash
# Build (native)
cd contracts/market
cargo build

# Build the Soroban WASM artifact
cargo build --target wasm32-unknown-unknown --release

# Test
cargo test
```

The WASM artifact lands at
`target/wasm32-unknown-unknown/release/vatix_market_contract.wasm`.

### Web app (`apps/web`)

The web app is a Next.js project under `apps/web`.

```bash
cd apps/web
npm install      # or pnpm install / yarn install, matching the lockfile
npm run build
npm test         # if a test script is defined in package.json
```

If a command above does not match the current `package.json` scripts or
`.github/workflows/ci.yml`, treat CI as the source of truth and update this
section in the same PR.

## Localnet deploy contributor path (#893)

End-to-end path for building, deploying, initializing, and smoke-verifying
the `market` contract on a local Soroban network. Follow it before opening a
PR that touches deploy scripts, constructor/`initialize` logic, or admin
surfaces — it is the fastest way to reproduce a contributor-reported bug
without touching testnet or mainnet.

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
error rather than partially applying

## Before opening a PR

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
- Propagate a 
  error rather than partially app

/* … truncated 1411 chars — edit only what you need near the top … */
