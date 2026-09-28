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

## Env examples are secret-free

Every `*.env.example` / `.env*.example` file in this repo is a **template of
placeholders only**. It must never contain a real secret, token, private key,
or funded account seed — not even a testnet one, since testnet keys are still
credentials and get scraped. Contributors copy the example to a real env file
and fill in their own values locally.

Rules for env examples:

1. **Placeholders only.** Use obvious non-secret placeholders (e.g.
   `S...` / `C...` / `https://...`), never a value that could sign a
transaction or authenticate to a service.
2. **Public vs secret is labelled.** `NEXT_PUBLIC_*` values are shipped to the
   browser and are public; everything else (secret keys, RPC auth tokens,
   server-only config) is secret and must stay out of the client bundle and
   out of git.
3. **Testnet vs mainnet is labelled.** Defaults must point at **testnet**.
   Never ship a default that silently targets mainnet — mainnet is opt-in and
   must be set explicitly.
4. **Fail closed.** An unset or placeholder value means "not configured";
   tooling must refuse to act (especially on money paths) rather than fall
   back to a mainnet endpoint or a shared key.
5. **Keys match the code.** Env example keys must match what the code actually
   reads (contract client, Soroban RPC, wallet config) so a contributor can
   run locally without guessing.

See `apps/web/.env.local.example` for the web app template and the
`NEXT_PUBLIC_*` contract IDs it expects (which should match the registry
above).

## Mainnet address review rule

Mainnet contract IDs are money-path configuration: a wrong or swapped ID
silently routes user funds to the wrong contract. Any change that adds or
edits a mainnet registry (e.g. `deployments/mainnet.json`) or a mainnet
`NEXT_PUBLIC_*` contract ID must follow these rules:

1. **Two-person review.** At least one maintainer other than the author
   approves the PR; self-merge of mainnet address changes is not allowed.
2. **On-chain verification.** The PR description lists, for every changed
   entry, the contract ID, the network passphrase
   (`Public Global Stellar Network ; September 2015`), and the WASM hash
   from `bash scripts/verify-wasm-hash.sh <contract-dir>`, and the reviewer
   confirms the hash matches the on-chain instance and
   `scripts/upgrade/expected-hashes.json`.
3. **No testnet drift.** A mainnet entry must never reuse a `testnet.json`
   contract ID or `rpcUrl`/`networkPassphrase`; reviewers reject any mix.
4. **Fail closed.** Empty or unreviewed mainnet IDs mean "not configured";
   tooling must refuse to target mainnet rather than fall back to testnet.
5. **Rollback.** Revert the registry PR to restore the previous IDs; the
   previous values must be recoverable from git history, never overwritten
   out-of-band.
