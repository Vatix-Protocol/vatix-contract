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
