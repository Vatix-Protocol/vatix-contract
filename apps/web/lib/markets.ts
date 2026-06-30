import { fetchContractMarkets } from "./soroban";

export type MarketStatus = "open" | "resolved" | "expired";

export interface Market {
  id: string;
  question: string;
  yesPrice: number;
  noPrice: number;
  volume: string;
  status: MarketStatus;
  endsAt: string;
}

/**
 * Fetch markets from the indexer API.
 * Returns an empty array when the indexer URL is not configured.
 */
export async function getMarkets(): Promise<Market[]> {
  const { markets } = await fetchContractMarkets();
  return markets.map((m) => ({
    id: m.id,
    question: m.question,
    yesPrice: m.yes_price,
    noPrice: m.no_price,
    volume: m.volume,
    status: m.status as MarketStatus,
    endsAt: m.ends_at,
  }));
}
