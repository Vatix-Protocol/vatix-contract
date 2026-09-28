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
