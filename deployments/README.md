# Deployments Registry

This directory holds machine-readable registries of deployed contract
instances, keyed by network. It is the **single source of truth** for
contract IDs that `apps/web` and the `scripts/` tooling should reference —
prefer reading from here over hardcoding IDs in multiple places.

## `testnet.json`

Registry of Stellar testnet contract instances.

### Schema

```jsonc
{
  "network": "testnet",
  "networkPassphrase": "Test SDF Network ; September 2015",
  "rpcUrl": "https://soroban-testnet.stellar.org",
  "contracts": {
    "market": { "contractId": "", "wasmHash": "" },
    "treasury": { "contractId": "", "wasmHash": "" },
    "resolution": { "contractId": "", "wasmHash": "" },
    "outcomeToken": { "contractId": "", "wasmHash": "" }
  }
}
```

Since JSON has no comment syntax, field meanings are documented here instead:

| Field | Meaning |
|---|---|
| `network` | Human-readable network name (`testnet`). |
| `networkPassphrase` | Soroban/Stellar network passphrase used to sign transactions for this network. |
| `rpcUrl` | Soroban RPC endpoint for this network. |
| `contracts.<name>.contractId` | The deployed contract's Stellar contract ID (`C...`). **Placeholder empty string (`""`) until a real deploy has happened** — fill this in after running `scripts/deploy-testnet.sh` (or an equivalent deploy) and recording the resulting contract ID. |
| `contracts.<name>.wasmHash` | Optional: the SHA-256 hash of the deployed WASM artifact (see `scripts/verify-wasm-hash.sh`), useful for confirming which build is live on-chain. Also a placeholder until filled in. |

### Filling in placeholders after a deploy

1. Deploy the contract (e.g. `TESTNET_SECRET_KEY=... bash scripts/deploy-testnet.sh`).
2. Copy the printed contract ID into the matching `contractId` field above.
3. Optionally record the WASM hash via `bash scripts/verify-wasm-hash.sh <contract-dir>` in `wasmHash`.
4. Commit the update so downstream consumers (web app, scripts) pick up the new ID.

### Consumers

- `apps/web/.env.local.example` points here as the canonical registry for
  testnet contract IDs (the web app itself still reads its actual IDs from
  `NEXT_PUBLIC_*` env vars at build/runtime — this file is the reference for
  what to put in them).
- `scripts/testnet-smoke.sh` reads this file as a fallback when the
  corresponding `*_CONTRACT_ID` environment variable isn't set.

Until real contracts are deployed, `contractId` values are empty strings and
any tooling that depends on them should treat that as "not yet configured"
rather than a valid address.
