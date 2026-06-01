# Scripts

## Deployment

- [`deploy.sh`](../deploy.sh) — echo guard; documents what a real mainnet deploy should do (build → deploy via Soroban CLI → log contract ID). The guard comment at the top of the file notes every step the real implementation must carry out. The corresponding CI step (`Deploy (dry-run guard)`) runs this script on every push to verify it is reachable and executable.
- [`deploy-testnet.sh`](../deploy-testnet.sh) — echo guard; documents what a real testnet deploy should do (build → deploy via Soroban CLI → log contract ID). The CI step (`Deploy testnet (guard)`) runs this script on every push so the deployment path is always exercised in CI.
- [`invoke-example.sh`](../invoke-example.sh) — echo guard; demonstrates the `stellar contract invoke` pattern for smoke-testing a deployed contract function. The CI step (`Invoke example (echo guard)`) runs this script on every push. The echo guard comment inside the script notes what the real implementation must do once a contract ID is available.

### invoke-example.sh usage

`invoke-example.sh` shows how to call a function on a deployed Soroban contract.
Replace the `echo` with the real invocation once a contract has been deployed and
`CONTRACT_ID` is set (e.g. exported from the deploy step):

```bash
# Smoke-test the hello function on testnet
export CONTRACT_ID="CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"

stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network testnet \
  --fn hello \
  --arg --world
```

The script is intentionally kept as an echo guard until testnet credentials are
available as repository secrets. See the CI step `Invoke example (echo guard)` in
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) for where it runs.

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

## Tooling config

- **rustfmt** — formatting rules for the market contract live in [`contracts/market/rustfmt.toml`](../../contracts/market/rustfmt.toml). The file currently contains only an echo-guard comment explaining what a real implementation should define; add explicit rules there when formatting conventions are agreed upon.
- **Contract Makefile** — [`contracts/market/Makefile`](../../contracts/market/Makefile) provides `build`, `test`, `fmt`, and `clean` targets. Each target currently includes an echo-guard comment explaining what the real implementation should do. Replace these guards with the actual commands once the build pipeline is finalized.
