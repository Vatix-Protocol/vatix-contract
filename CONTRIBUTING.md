# Contributing to vatix-contract

Guide for adding a new contract crate or working on an existing one
(`market`, `treasury`, `resolution`, `outcome-token`).

## Workspace layout

- Each contract lives under `contracts/<name>/` as its own crate with its
  own `Cargo.toml`, `src/lib.rs`, `src/test.rs` (or inline `#[cfg(test)]`
  modules), and `rustfmt.toml`/`Makefile` where needed.
- Root `Cargo.toml` uses `members = ["contracts/*"]`, so any new directory
  under `contracts/` is picked up automatically — no manual registration
  needed. Shared workspace dependency versions (e.g. `soroban-sdk`,
  `ed25519-dalek`) are pinned once in `[workspace.dependencies]`; prefer
  `{ workspace = true }` over restating a version in the crate's own
  `Cargo.toml`.
- Cross-contract integration tests live at the repo root under `tests/`
  and are compiled as the `vatix-contract-tests` package (see root
  `Cargo.toml`).

## Soroban contract patterns

- `#![no_std]` at the crate root; use `soroban_sdk` types (`Vec`, `String`,
  `Address`, ...) instead of `std` equivalents in contract code. `std` is
  only pulled in inside `#[cfg(test)]` modules that need it (e.g. to read
  a fixture file).
- Public contract functions return `Result<T, ContractError>` — add new
  error variants to `error.rs` rather than panicking.
- Release builds compile with `panic = "abort"` (workspace `[profile.release]`
  in the root `Cargo.toml`) since WASM has no stack-unwinding support — see
  [README.md § Panic Strategy](README.md#panic-strategy-soroban-contract-builds)
  for the smoke-test command and why `cargo test`'s `catch_unwind`-based
  assertions don't carry over to a deployed contract.
- Emit a `#[contractevent]` (see `events.rs` in any contract) for state
  changes that off-chain indexers care about, following the existing
  `EVENT_VERSION` topic-versioning pattern.
- Validate inputs in `validation.rs` and keep math (fees, shares, payouts)
  in small, pure, unit-testable functions.

## Before opening a PR

```bash
# Format (per contract, uses that contract's rustfmt.toml)
cd contracts/<name> && cargo fmt --check

# Lint
cd contracts/<name> && cargo clippy -- -D warnings

# Unit tests for the crate you touched
cd contracts/<name> && cargo test

# Workspace-wide compile check — catches drift between crates (#503)
cargo check --workspace --all-targets

# Workspace integration tests
cargo test --workspace --tests
```

See [README.md § Workspace compile check](README.md#workspace-compile-check)
for what that job guards against, [README.md § Clippy Lints](README.md#clippy-lints)
for the lint policy, and `contracts/market/STORAGE_MIGRATION_GUIDE.md` /
`contracts/market/MIGRATION.md` if your change touches persistent storage
shape. If you add a regression test vector for math logic, add it to
`test-vectors/` alongside a Rust test that loads and asserts it (see
`contracts/market/src/tests_vectors.rs` for the pattern).

## PR / issue hygiene

- Keep PRs scoped to one issue; note in the PR description which
  acceptance criteria it satisfies.
- If an issue's scope is unclear, ask in the issue thread before
  implementing.
- Don't fix unrelated pre-existing issues in the same PR — file/flag them
  separately instead.
