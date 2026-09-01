import { getMarkets, type Market } from "./markets";

export async function findMarketById(id: string): Promise<Market | undefined> {
  const markets = await getMarkets();
  return markets.find((market) => market.id === id);
}
