/**
 * Soroban / indexer helpers.
 *
 * All config comes from Next.js public env vars.
 */

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
    const res = await fetch(`${INDEXER_API_URL}/markets`, {
      next: { revalidate: 30 },
    });
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
export { invokeContract } from "./contract-client";
