import { MOCK_MARKETS, type Market } from "./markets";

export function findMarketById(id: string): Market | undefined {
  return MOCK_MARKETS.find((market) => market.id === id);
}
