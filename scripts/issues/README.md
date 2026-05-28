# Scripts

## Deployment

- [`deploy.sh`](../deploy.sh) — deploys the compiled contract to the configured network.
- [`deploy-testnet.sh`](../deploy-testnet.sh) — echo guard; documents what a real testnet deploy should do (build → deploy via Soroban CLI → log contract ID).
- [`invoke-example.sh`](../invoke-example.sh) — example invocation of a deployed contract function.

---

# Contributor issue generator

Generates **375** onboarding issues (**125** per repo) for:

- `Vatix-Protocol/vatix-contract` (frontend + contracts + tooling)
- `Vatix-Protocol/vatix-backend`
- `Vatix-Protocol/Swyft`

## Usage

From the repo root:

```bash
pnpm install
pnpm issues:generate          # writes JSON to scripts/issues/generated/
pnpm issues:publish           # generate + create on GitHub (requires gh auth)
```

### Options

```bash
pnpm issues:generate -- --help
pnpm issues:generate -- --repo vatix-backend
pnpm issues:generate -- --publish --delay-ms 500
```

- **`--publish`** — creates issues via `gh issue create` (skips titles that already exist)
- **`--repo`** — limit to one repo slug: `vatix-contract`, `vatix-backend`, `swyft`
- **`--delay-ms`** — pause between GitHub API calls (default `300`)
- **`--dry-run`** — print sample issues without writing files

## Output

| File | Issues |
|------|--------|
| `generated/vatix-contract.json` | 125 |
| `generated/vatix-backend.json` | 125 |
| `generated/swyft.json` | 125 |
| `generated/manifest.json` | summary |

Generated JSON is gitignored; re-run anytime for a fresh local export.
