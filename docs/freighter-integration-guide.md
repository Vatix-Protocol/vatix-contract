# Freighter Integration Guide for Contributors

This guide explains how the Vatix web frontend integrates
[Freighter](https://freighter.app) — the Stellar browser-extension wallet —
with the Soroban smart contracts. Read it before touching anything in
`apps/web/context/`, `apps/web/lib/`, or any component that sends a
transaction.

---

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Architecture](#architecture)
4. [WalletContext — connection state](#walletcontext--connection-state)
5. [Signing and submitting transactions](#signing-and-submitting-transactions)
6. [ScVal helpers](#scval-helpers)
7. [Environment variables](#environment-variables)
8. [Stale / disconnected / invalid states](#stale--disconnected--invalid-states)
9. [Adding a new contract call](#adding-a-new-contract-call)
10. [Generating TypeScript bindings](#generating-typescript-bindings)
11. [CI integration](#ci-integration)
12. [Common errors and fixes](#common-errors-and-fixes)
13. [Network Mismatch Errors](#network-mismatch-errors-issue-587)

---

## Overview

Freighter is loaded **on the client only** via a dynamic import so Next.js SSR
never crashes on `window` access. The flow for every mutating operation is:

```
User action
  → connect wallet (Freighter getAddress)
  → build unsigned transaction (stellar-sdk)
  → simulate via Soroban RPC (get resource fees + footprint)
  → sign with Freighter (signTransaction)
  → submit via Soroban RPC (sendTransaction)
  → poll for confirmation (getTransaction)
```

Read-only queries skip signing and use a dummy source account purely for
simulation purposes.

---

## Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| Node.js | 20+ | Runtime |
| pnpm | 8+ | Package manager |
| Freighter browser extension | latest | Wallet signing |
| `@stellar/freighter-api` | ^3.1.0 (pinned in `package.json`) | Freighter JS bindings |
| `@stellar/stellar-sdk` | 16.0.1 (pinned) | Transaction building, XDR encoding |

Install extension: <https://freighter.app>

Install dependencies (do **not** change versions without a review):

```bash
pnpm install
```

---

## Architecture

```
apps/web/
├── context/
│   └── WalletContext.tsx      # React context — wallet address + connect/disconnect
├── lib/
│   ├── contract-client.ts     # Generic invoke/query helpers + ScVal converters
│   └── soroban.ts             # Network config, RPC server, lightweight invoke helper
└── components/
    ├── WalletConnectButton.tsx # Pure presentational button — no Freighter calls
    ├── DepositForm.tsx         # Example of a component that calls a contract method
    └── WithdrawForm.tsx
```

`WalletContext` owns **only** the address and connection state.  
`contract-client.ts` owns **all** transaction logic.  
Components call `useWallet()` for the address and import from `contract-client.ts`
for the actual invocation.

---

## WalletContext — connection state

**File:** `apps/web/context/WalletContext.tsx`

```tsx
// Simplified — see the actual file for full implementation
const connect = useCallback(async () => {
  setIsConnecting(true);
  setConnectError(null);
  try {
    // Dynamic import keeps this SSR-safe
    const { isConnected, getAddress } = await import("@stellar/freighter-api");

    const connectedResult = await isConnected();
    if (connectedResult.error || !connectedResult.isConnected) {
      // Extension not installed, or installed but locked/not connected.
      // Fail closed: surface a clear error, never invent an address.
      setConnectError("Freighter is not connected. Install/unlock it.");
      setAddress(null);
      return;
    }

    const result = await getAddress();
    if (result.error || !result.address) {
      setConnectError("Failed to get wallet address from Freighter.");
      return;
    }
    setAddress(result.address);
  } catch (err) {
    // Freighter API unavailable or threw — fail closed with a visible error,
    // never a fallback stub address (Issue #700).
    setConnectError(`Freighter unavailable: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    setIsConnecting(false);
  }
}, []);
```

### Rules for contributors

- **Never import `@stellar/freighter-api` at the top level** of any module.
  Always use `await import(...)` inside an async function. This keeps Next.js
  server-side rendering working and prevents `window is not defined` errors.
- **Fail closed — never a fallback stub address (Issue #700).** Every
  failure path in `connect()` (extension not installed, locked, user
  rejected, no address returned, unexpected throw) must set `address` to
  `null` and populate `connectError` with a user-facing message. A
  hardcoded/placeholder address (the old `GSTUB…` stub) silently masks
  misconfiguration: the UI looks "connected" while every subsequent
  contract call either fails with a confusing on-chain error or, worse,
  succeeds against the wrong account. `scripts/check-no-wallet-fallback-stub.sh`
  enforces this in CI — it fails the build if `GSTUB` or a string-literal
  `setAddress(...)` call ever reappears under `apps/web/`.
- Consume `useWallet()` in components; never import `WalletContext` directly.

```tsx
import { useWallet } from "@/context/WalletContext";

function MyComponent() {
  const { address, isConnecting, connect, disconnect } = useWallet();
  // ...
}
```

---

## Signing and submitting transactions

**File:** `apps/web/lib/contract-client.ts`

`invokeContract` is the standard entry point for any state-changing call:

```typescript
import {
  invokeContract,
  MARKET_CONTRACT_ID,
  amountToScVal,
  addressToScVal,
  u32ToScVal,
} from "@/lib/contract-client";

const result = await invokeContract(
  MARKET_CONTRACT_ID,        // contract address from env
  "deposit_collateral",      // method name (must match contract exactly)
  [
    u32ToScVal(marketId),
    addressToScVal(userAddress),
    amountToScVal(amountInStroops),
  ],
  userAddress                // signs with Freighter as this address
);
// result: { hash: string, status: string }
```

### What happens inside `invokeContract`

1. **Load account** — fetches the sequence number from Horizon so the
   transaction can be built with the correct nonce.
2. **Build transaction** — wraps the contract call in an
   `InvokeHostFunctionOperation` via `stellar-sdk`.
3. **Simulate** — sends to the Soroban RPC (`simulateTransaction`). This fills
   in the resource fee and the read/write footprint that Soroban requires.
   The call fails here if the operation would revert on-chain.
4. **Assemble** — `SorobanRpc.assembleTransaction` merges simulation results
   into the transaction XDR.
5. **Sign with Freighter** — `signTransaction(preparedTx.toXDR(), { networkPassphrase, address })`.
   Freighter opens its popup; the user approves or rejects.
6. **Submit** — `server.sendTransaction(signedTx)`.
7. **Poll** — `server.getTransaction(hash)` is called every 1 s, up to 20
   attempts, waiting for `SUCCESS` or `FAILED`.

### Read-only queries

Use `queryContract` for view functions. It skips Freighter entirely:

```typescript
import { queryContract, MARKET_CONTRACT_ID } from "@/lib/contract-client";
import { u32ToScVal } from "@/lib/contract-client";

const marketData = await queryContract(
  MARKET_CONTRACT_ID,
  "get_market",
  [u32ToScVal(marketId)],
);
```

A dummy source account (`GAAAA…WHF`) is used purely to satisfy the SDK's
transaction-builder API. No signing happens.

---

## ScVal helpers

Soroban contract arguments must be encoded as XDR `ScVal` values. The helpers
in `contract-client.ts` cover the most common types used by the Vatix contracts:

| Helper | Rust type | Notes |
|---|---|---|
| `amountToScVal(amount)` | `i128` | Splits into hi/lo `u64` correctly. Pass stroops (1 USDC = 10 000 000). |
| `addressToScVal(address)` | `Address` | Accepts a strkey `G…` string. Uses `new Address(address).toScVal()` from stellar-sdk for correct base32 decoding. |
| `u32ToScVal(value)` | `u32` | For market IDs and other 32-bit unsigned integers. |
| `boolToScVal(value)` | `bool` | For `outcome` flags in `resolve_market`. |
| `stringToScVal(value)` | `String` | For `question` and other contract string fields. |

> **Important:** `resolve_market` on the Rust side takes `market_id` as
> `soroban_sdk::String` (not `u32`). Use `stringToScVal(String(marketId))`
> for that specific call. All other market methods take `u32`.

---

## Environment variables

`deployments/testnet.json` is the single source of truth for deployed
contract IDs (see `deployments/README.md`). Don't hand-copy values out of it
— run the sync script instead, which writes them straight into `.env.local`
(Issue #700):

```bash
# From the repo root — creates .env.local from .env.local.example if it
# doesn't exist yet, then writes every contract ID/RPC URL/passphrase
# currently recorded in deployments/testnet.json into it.
pnpm env:sync
```

Re-run `pnpm env:sync` after every redeploy that updates
`deployments/testnet.json` — a stale ID in `.env.local` doesn't fail loudly,
it just makes contract calls throw confusing errors (or point at a
decommissioned contract), which is exactly the class of silent
misconfiguration this project fails closed against (#700). The script never
blanks out an existing value with an empty registry placeholder — it only
writes fields that have a real deployed ID.

`.env.local` ends up with:

```env
# Contract addresses (synced from deployments/testnet.json via `pnpm env:sync`)
NEXT_PUBLIC_MARKET_CONTRACT_ID=C...
NEXT_PUBLIC_TREASURY_CONTRACT_ID=C...
NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID=C...
NEXT_PUBLIC_RESOLUTION_CONTRACT_ID=C...

# Soroban RPC (defaults to Stellar public testnet)
NEXT_PUBLIC_SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Network passphrase (must match what Freighter is configured to use)
NEXT_PUBLIC_NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# Horizon (for account sequence number lookup)
NEXT_PUBLIC_HORIZON_URL=https://horizon-testnet.stellar.org
```

**Never commit `.env.local`.** It is listed in `.gitignore`.

The passphrase in `.env.local` **must** match the network Freighter is
connected to. If they differ, Freighter will reject the signing request.
Testnet passphrase: `Test SDF Network ; September 2015`.
Mainnet passphrase: `Public Global Stellar Network ; September 2015`.

---

## Stale / disconnected / invalid states

The following edge cases are already handled in the existing code. Preserve
these behaviors in any new code:

| Situation | Behavior |
|---|---|
| Freighter extension not installed | `connect()` catches the import/call error, sets `address` to `null`, and populates `connectError` with a user-visible message. Fails closed — never a fallback stub address (#700). |
| Extension installed but locked / user not connected | `isConnected()` returns `false`; address is set to `null`, `connectError` is populated, and the connect button stays active. |
| User rejects the signing popup | `signTransaction` returns `{ error: ... }`; `invokeContract` throws with `Freighter signing failed: <reason>`. Components must catch and display this. |
| Transaction fails on-chain | `sendTransaction` returns `status: "ERROR"` or `getTransaction` returns `FAILED`; `invokeContract` throws with the result XDR. |
| Polling timeout (>20 s) | Loop exits after 20 attempts; the function returns the last `hash` and `status`. The transaction may still confirm later. |
| Contract ID not configured | `invokeContract` and `queryContract` throw immediately with a human-readable message pointing to `.env.local`. |
| Invalid Stellar address passed to `addressToScVal` | `new Address(address)` throws a `TypeError`; bubble the error to the component's error state. |

Pattern to follow in components:

```tsx
const [error, setError] = useState<string | null>(null);
const [txHash, setTxHash] = useState<string | null>(null);

const handleAction = async () => {
  if (!address) {
    setError("Connect your Freighter wallet first.");
    return;
  }
  setError(null);
  try {
    const { hash } = await invokeContract(...);
    setTxHash(hash);
  } catch (err) {
    setError(err instanceof Error ? err.message : "Transaction failed.");
  }
};
```

---

## Adding a new contract call

1. **Identify the method signature** in `contracts/market/src/lib.rs`.
2. **Map Rust types to ScVal helpers** using the table in [ScVal helpers](#scval-helpers).
3. **Call `invokeContract`** (state-changing) or `queryContract` (read-only)
   from `contract-client.ts`.
4. **Handle errors** in the component's catch block (see pattern above).

Example — calling `update_position`:

```typescript
// Rust signature:
//   update_position(env, user: Address, market_id: u32,
//                   yes_delta: i128, no_delta: i128, market_price: i128) → Position

import {
  invokeContract,
  MARKET_CONTRACT_ID,
  addressToScVal,
  u32ToScVal,
  amountToScVal,
} from "@/lib/contract-client";

await invokeContract(
  MARKET_CONTRACT_ID,
  "update_position",
  [
    addressToScVal(userAddress),
    u32ToScVal(marketId),
    amountToScVal(yesDelta),   // i128 — positive to buy, negative to sell
    amountToScVal(noDelta),
    amountToScVal(marketPriceBps),
  ],
  userAddress,
);
```

---

## Generating TypeScript bindings

Auto-generated bindings live in `apps/web/lib/contracts/`. They are produced
from the compiled WASM files and **must not be edited by hand**.

```bash
# From the repo root — builds all contracts then generates TS clients
pnpm build:bindings
```

Regenerate after any contract change and commit the updated files. The CI
`frontend` job runs `pnpm build:bindings` before lint and build, so a
stale binding will fail the pipeline.

See `apps/web/lib/contracts/README.md` for full details.

---

## CI integration

The `frontend` job in `.github/workflows/ci.yml` validates the full Freighter
integration path on every push and PR:

```
pnpm install
  → pnpm build:bindings                              # builds WASM + generates TS clients
  → bash scripts/check-no-wallet-fallback-stub.sh     # fails closed: no GSTUB fallback (#700)
  → pnpm --filter web lint                            # tsc --noEmit (type-checks all contract-client helpers)
  → pnpm --filter web build                           # Next.js production build
```

A TypeScript error in `contract-client.ts`, `WalletContext.tsx`, or any
component that uses them will fail the `lint` step. Fix type errors locally
before pushing:

```bash
cd apps/web && pnpm lint
```

---

## Common errors and fixes

### `window is not defined` during `next build`

**Cause:** Top-level import of `@stellar/freighter-api` at module scope.  
**Fix:** Move the import inside an `async` function using `await import(...)`.

---

### `Freighter signing failed: User declined signing`

**Cause:** User clicked "Reject" in the Freighter popup, or the popup was
closed before approval.  
**Fix:** Surface the error message to the user; do not retry automatically.

---

### `Simulation failed: HostError: Error(Contract, ...)`

**Cause:** The transaction would revert on-chain. Common reasons:
- Wrong argument types / order.
- Market not active (`MarketNotActive`).
- Insufficient collateral (`InsufficientCollateral`).
- Calling an admin-only method from a non-admin address.

**Fix:** Check the contract error enum in `contracts/market/src/error.rs` and
validate inputs client-side before invoking.

---

### `Contract ID not configured`

**Cause:** `NEXT_PUBLIC_MARKET_CONTRACT_ID` (or another contract env var) is
empty in `.env.local`.  
**Fix:** Copy `.env.local.example` to `.env.local` and fill in the contract
addresses from your testnet deployment.

---

### `Invalid Stellar address` / `TypeError` from `addressToScVal`

**Cause:** A non-strkey string (e.g. hex or base64) was passed to
`addressToScVal`.  
**Fix:** Always pass a `G…` strkey address as returned by `getAddress()` from
Freighter. Validate with `StrKey.isValidEd25519PublicKey(address)` from
`stellar-sdk` before invoking if the address comes from user input.

---

### Network passphrase mismatch

**Cause:** `NEXT_PUBLIC_NETWORK_PASSPHRASE` in `.env.local` does not match
the network Freighter is connected to.  
**Fix:** Ensure both are set to the same value. Testnet:
`Test SDF Network ; September 2015`.

See the dedicated section below for the full troubleshooting guide.

---

## Network Mismatch Errors (Issue #587)

Freighter enforces that the network passphrase used to build a transaction
matches the network the extension is currently connected to. When they differ
the signing popup is skipped and `signTransaction` returns an error. This is
one of the most common integration failures when switching between testnet and
mainnet or when a contributor's `.env.local` is misconfigured.

### How a mismatch happens

```
App builds transaction with passphrase: "Test SDF Network ; September 2015"
                        ↓
Freighter is connected to: "Public Global Stellar Network ; September 2015"
                        ↓
Freighter rejects: "Transaction is not for the connected network"
```

The passphrase is baked into the transaction XDR before it reaches Freighter,
so the mismatch is caught at signing time — not during simulation.

### Recognised network passphrases

| Network | Passphrase |
|---|---|
| Testnet | `Test SDF Network ; September 2015` |
| Mainnet | `Public Global Stellar Network ; September 2015` |
| Futurenet | `Test SDF Future Network ; October 2022` |
| Local standalone | `Standalone Network ; February 2017` |

### Diagnosing the error

When `invokeContract` throws with a message like:

```
Freighter signing failed: Transaction is not for the connected network
```

or the Freighter popup shows **"Wrong network"**, work through this checklist:

1. **Check `.env.local`** — open `apps/web/.env.local` and confirm
   `NEXT_PUBLIC_NETWORK_PASSPHRASE` is set to the passphrase of the network
   you intend to use.

2. **Check Freighter's active network** — click the Freighter extension icon
   and look at the network name displayed in the top-right corner of the
   popup. It must match the passphrase in step 1.
   - To switch networks in Freighter: open the extension → click the network
     name → select the target network from the dropdown.

3. **Check `soroban.ts`** — open `apps/web/lib/soroban.ts` and verify the
   `NETWORK_PASSPHRASE` constant (or the value read from
   `process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE`) matches both `.env.local`
   and Freighter.

4. **Hard-reload the page** — Next.js hot-reload may cache a stale env value.
   Stop the dev server, re-run `pnpm dev`, and reload the browser tab.

5. **Check `signTransaction` call** — the call in `contract-client.ts` must
   pass `networkPassphrase` explicitly:

   ```typescript
   const signed = await signTransaction(preparedTx.toXDR(), {
     networkPassphrase: NETWORK_PASSPHRASE, // must equal the tx passphrase
     address: sourceAddress,
   });
   ```

   If `networkPassphrase` is omitted or `undefined`, Freighter defaults to
   mainnet and will reject any testnet transaction.

### Quick fix for the most common case

```bash
# 1. Confirm which network your testnet contract is deployed on:
cat deployments/testnet.json | grep networkPassphrase

# 2. Copy the passphrase into .env.local:
echo 'NEXT_PUBLIC_NETWORK_PASSPHRASE=Test SDF Network ; September 2015' \
  >> apps/web/.env.local

# 3. Restart the dev server:
pnpm dev
```

Then in Freighter, switch to **Testnet** using the network dropdown.

### Preventing mismatches in CI / deployment

- The `NEXT_PUBLIC_NETWORK_PASSPHRASE` in `apps/web/.env.local.example`
  defaults to the testnet value. Never change it to mainnet in that file.
- Mainnet deployments must set `NEXT_PUBLIC_NETWORK_PASSPHRASE` as a
  CI/CD secret (e.g. a GitHub Actions environment secret), never in source.
- The passphrase in `deployments/testnet.json` (field `networkPassphrase`)
  is the canonical reference; keep it consistent with `.env.local.example`.

---

*This document lives in `docs/freighter-integration-guide.md`. Update it
when the integration layer changes.*
