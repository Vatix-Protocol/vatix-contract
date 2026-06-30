/**
 * Soroban / indexer helpers.
 *
 * All config comes from Next.js public env vars.
 */

import {
  Contract,
  nativeToScVal,
  rpc,
  TransactionBuilder,
} from "@stellar/stellar-sdk";
import { addressToScVal, amountToScVal } from "./contract-client";

export const CONTRACT_ID = process.env.NEXT_PUBLIC_CONTRACT_ID ?? "";

export const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ??
  "https://soroban-testnet.stellar.org";

export const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  "Test SDF Network ; September 2015";

export const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ??
  "https://horizon-testnet.stellar.org";

export const INDEXER_API_URL =
  process.env.NEXT_PUBLIC_INDEXER_API_URL ?? "";

// Re-export MARKET_CONTRACT_ID so existing callers don't need to change imports.
export { MARKET_CONTRACT_ID } from "./contract-client";

// ---------------------------------------------------------------------------
// Market reads (indexer API)
// ---------------------------------------------------------------------------

export interface RpcMarket {
  id: string;
  question: string;
  yes_price: number;
  no_price: number;
  volume: string;
  status: string;
  ends_at: string;
}

interface GetMarketsResult {
  markets: RpcMarket[];
}

/**
 * Fetch markets from the backend indexer API.
 * Falls back to an empty array on error or when the URL is not configured.
 */
export async function fetchContractMarkets(): Promise<GetMarketsResult> {
  if (!INDEXER_API_URL) {
    return { markets: [] };
  }
  try {
    const server = new rpc.Server(SOROBAN_RPC_URL);
    // `getContractData` is a low-level key/value read; the actual key depends
    // on your indexer. Adjust StorageKey to match your contract's storage layout.
    const result = await server.getContractData(
      CONTRACT_ID,
      nativeToScVal("MARKETS"),
      rpc.Durability.Persistent,
    );
    if (!result || !result.val) {
      return { markets: [] };
    }
    const data = await res.json();
    // Accept both `{ markets: [...] }` and a bare array from the indexer.
    const markets: RpcMarket[] = Array.isArray(data) ? data : (data.markets ?? []);
    return { markets };
  } catch {
    return { markets: [] };
  }
}

// ---------------------------------------------------------------------------
// invokeContract re-exported from contract-client for backward compat
// ---------------------------------------------------------------------------

interface SendResult {
  hash: string;
  status: string;
}

/**
 * Invoke a contract function using Freighter for signing.
 *
 * Flow:
 *   1. Fetch the caller's account from Horizon (sequence number)
 *   2. Build an unsigned InvokeHostFunction transaction via stellar-sdk
 *   3. Simulate via Soroban RPC (fills resource fees)
 *   4. Sign with Freighter
 *   5. Submit via Soroban RPC
 */
export async function invokeContract(
  functionName: "deposit" | "withdraw",
  args: { amount: string; address: string },
): Promise<{ hash: string }> {
  if (!CONTRACT_ID) {
    throw new Error(
      "NEXT_PUBLIC_CONTRACT_ID is not set. Add it to your .env.local file.",
    );
  }

  const server = new rpc.Server(SOROBAN_RPC_URL);

  // 1. Load source account (needed for sequence number)
  const account = await server.getAccount(args.address);

  // 2. Build the transaction
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(
      contract.call(
        functionName,
        addressToScVal(args.address),
        amountToScVal(args.amount),
      ),
    )
    .setTimeout(30)
    .build();

  // 3. Simulate to get resource fees and footprint
  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  const preparedTx = rpc.assembleTransaction(tx, sim).build();

  // 4. Sign with Freighter
  const { signTransaction } = await import("@stellar/freighter-api");
  const { signedTxXdr, error } = await signTransaction(preparedTx.toXDR(), {
    networkPassphrase: NETWORK_PASSPHRASE,
    address: args.address,
  });
  if (error) throw new Error(error.message);

  // 5. Submit
  const sent = await server.sendTransaction(
    TransactionBuilder.fromXDR(signedTxXdr, NETWORK_PASSPHRASE),
  ) as SendResult;

  return { hash: sent.hash };
}
