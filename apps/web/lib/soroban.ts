/**
 * Soroban / indexer helpers.
 *
 * All config comes from Next.js public env vars.
 */

import {
  Contract,
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
// Freighter network passphrase resolution & validation
// ---------------------------------------------------------------------------

/**
 * Stable error codes for Freighter network passphrase handling.
 * Callers can branch on these without parsing messages.
 */
export type FreighterNetworkErrorCode =
  | "FREIGHTER_NETWORK_MISSING_CONFIG"
  | "FREIGHTER_NETWORK_UNAVAILABLE"
  | "FREIGHTER_NETWORK_MISMATCH";

export class FreighterNetworkError extends Error {
  readonly code: FreighterNetworkErrorCode;

  constructor(code: FreighterNetworkErrorCode, message: string) {
    super(message);
    this.name = "FreighterNetworkError";
    this.code = code;
  }
}

/**
 * Resolve the expected network passphrase from config.
 * Fail-closed: throws when the passphrase is not configured.
 */
export function resolveExpectedNetworkPassphrase(): string {
  const passphrase = NETWORK_PASSPHRASE.trim();
  if (!passphrase) {
    throw new FreighterNetworkError(
      "FREIGHTER_NETWORK_MISSING_CONFIG",
      "NEXT_PUBLIC_NETWORK_PASSPHRASE is not set. Configure the expected network passphrase.",
    );
  }
  return passphrase;
}

/**
 * Read the network passphrase currently selected in Freighter.
 * Fail-closed: throws when Freighter is unavailable or returns no passphrase.
 */
export async function getFreighterNetworkPassphrase(): Promise<string> {
  const { getNetworkDetails, error } = await import("@stellar/freighter-api");
  if (error) {
    throw new FreighterNetworkError(
      "FREIGHTER_NETWORK_UNAVAILABLE",
      error.message,
    );
  }
  const details = await getNetworkDetails();
  const passphrase = details?.networkPassphrase?.trim();
  if (!passphrase) {
    throw new FreighterNetworkError(
      "FREIGHTER_NETWORK_UNAVAILABLE",
      "Freighter did not report a network passphrase.",
    );
  }
  return passphrase;
}

/**
 * Assert that the connected Freighter network matches the configured network.
 * Deny-by-default: any mismatch (or missing config) rejects signing.
 */
export async function assertFreighterNetworkMatches(): Promise<string> {
  const expected = resolveExpectedNetworkPassphrase();
  const actual = await getFreighterNetworkPassphrase();
  if (actual !== expected) {
    throw new FreighterNetworkError(
      "FREIGHTER_NETWORK_MISMATCH",
      "Connected Freighter network does not match the configured network passphrase.",
    );
  }
  return expected;
}

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
  closed_to_deposits?: boolean;
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
    const res = await fetch(`${INDEXER_API_URL}/markets`);
    if (!res.ok) {
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
 *   1. Verify the connected Freighter network matches the configured network
 *   2. Fetch the caller's account from Horizon (sequence number)
 *   3. Build an unsigned InvokeHostFunction transaction via stellar-sdk
 *   4. Simulate via Soroban RPC (fills resource fees)
 *   5. Sign with Freighter
 *   6. Submit via Soroban RPC
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

  // 1. Fail-closed: reject when Freighter is on the wrong network.
  const networkPassphrase = await assertFreighterNetworkMatches();

  const server = new rpc.Server(SOROBAN_RPC_URL);

  // 2. Load source account (needed for sequence number)
  const account = await server.getAccount(args.address);

  // 3. Build the transaction
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase,
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

  // 4. Simulate to get resource fees and footprint
  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  const preparedTx = rpc.assembleTransaction(tx, sim).build();

  // 5. Sign with Freighter
  const { signTransaction } = await import("@stellar/freighter-api");
  const { signedTxXdr, error } = await signTransaction(preparedTx.toXDR(), {
    networkPassphrase,
    address: args.address,
  });
  if (error) throw new Error(error.message);

  // 6. Submit
  const sent = await server.sendTransaction(
    TransactionBuilder.fromXDR(signedTxXdr, networkPassphrase),
  ) as SendResult;

  return { hash: sent.hash };
}
