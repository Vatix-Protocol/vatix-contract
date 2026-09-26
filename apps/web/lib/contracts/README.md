# Contract Bindings

This directory contains auto-generated TypeScript bindings for Soroban smart contracts.

## Files

- `market.ts` - Market contract client
- `treasury.ts` - Treasury contract client
- `outcome-token.ts` - Outcome token contract client
- `resolution.ts` - Resolution contract client
- `index.ts` - Re-exports and environment configuration

## Generation

Bindings are generated from compiled WASM files using the Stellar CLI:

```bash
# Generate all contract bindings
pnpm build:bindings
```

This runs `scripts/generate-bindings.ts` which:
1. Builds all contracts to WASM
2. Generates TypeScript client code using `stellar contract bindings typescript`
3. Places output files in this directory

## Usage

Import the contract clients and use them with the contract client helpers:

```typescript
import { invokeContract, MARKET_CONTRACT_ID } from '@/lib/soroban';
import { amountToScVal, addressToScVal, u32ToScVal } from '@/lib/contract-client';

// Prepare arguments
const args = [
  u32ToScVal(marketId),
  addressToScVal(userAddress),
  amountToScVal(amount),
];

// Invoke contract method
const result = await invokeContract(
  MARKET_CONTRACT_ID,
  'deposit_collateral',
  args,
  userAddress
);
```

## Configuration

Contract IDs must be set in `.env.local`:

```env
NEXT_PUBLIC_MARKET_CONTRACT_ID=C...
NEXT_PUBLIC_TREASURY_CONTRACT_ID=C...
NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID=C...
NEXT_PUBLIC_RESOLUTION_CONTRACT_ID=C...
```

See `apps/web/.env.local.example` for all required environment variables.

## Development

These files are auto-generated. **Do not edit manually.**

To regenerate after contract changes:
1. Update the contract source code
2. Run `pnpm build:bindings`
3. Commit the generated files

## CI Integration

The CI pipeline automatically generates bindings before building the web app:

```yaml
- name: Generate TypeScript bindings
  run: pnpm build:bindings

- name: Build web app
  run: pnpm --filter web build
```

This ensures the web app always has up-to-date contract bindings.

## Sync Invariants (web bindings sync)

Web callers must not drift from on-chain signatures. The following invariants are
enforced by `scripts/generate-bindings.ts` and verified in CI:

1. **Signature parity** — every exported entrypoint in `index.ts` maps 1:1 to a
   `vatix-contract` entrypoint. Missing or extra exports fail the sync check.
2. **Stable error codes** — contract call failures surface typed error codes
   aligned with `apps/web/lib/errors.ts`. Do not introduce ad-hoc string errors.
3. **Correlation ids** — every money-path call (deposit, withdraw, swap,
   settle, resolve) carries a correlation id so failures are traceable across
   web, RPC, and contract logs.
4. **Deny-by-default authz** — privileged entrypoints (admin, treasury,
   resolution) require an explicit role check before invocation. Untrusted
   clients cannot bypass policy; the server/contract remains the source of
   truth for balances, swaps, and admin.
5. **Fail-closed writes** — if the RPC/DB/Redis dependency is unavailable, write
   paths fail closed rather than silently succeeding.
6. **Idempotency** — replayed or concurrent requests are deduplicated by
   correlation id so retries cannot double-apply money-path effects.

### Drift detection

Run the sync check locally before committing binding changes:

```bash
pnpm --filter web check:bindings
```

CI runs the same check as a required gate. A drift failure means the generated
bindings no longer match the current `vatix-contract` entrypoints — regenerate
with `pnpm build:bindings` and re-run the check.

### Observability

Money-path calls emit ops-safe metrics and structured logs (method, contract
id, correlation id, outcome). Secrets, private keys, and full payloads are
never logged. See `apps/web/lib/errors.ts` for the error-code taxonomy.

### Rollback / kill-switch

Any change that affects a money path or mainnet behavior must land behind a
feature flag with a documented kill-switch. Rollback steps belong in the PR
description; do not ship irreversible mainnet changes without the readiness
checklist.

### Testnet vs mainnet

Contract IDs differ per network. Never hardcode a mainnet address in web code;
read from environment configuration and validate the network before invoking.
Address drift between testnet and mainnet is a fail-closed condition.
