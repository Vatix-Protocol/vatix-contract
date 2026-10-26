import { getMarkets, type Market } from "./markets";

/**
 * Returns true only for the canonical market id form used by `getMarkets`:
 * a non-empty string of ASCII digits with no leading zeros, except the
 * single value `"0"`.
 */
export function isValidMarketId(id: string): boolean {
  if (id.length === 0) return false;
  if (!/^[0-9]+$/.test(id)) return false;
  if (id.length > 1 && id.startsWith("0")) return false;
  return true;
}

export type FindMarketResult =
  | { kind: "ok"; market: Market }
  | { kind: "invalid-id"; id: string }
  | { kind: "not-found"; id: string };

export async function findMarketById(id: string): Promise<FindMarketResult> {
  if (!isValidMarketId(id)) {
    return { kind: "invalid-id", id };
  }
  const markets = await getMarkets();
  const market = markets.find((m) => m.id === id);
  if (!market) {
    return { kind: "not-found", id };
  }
  return { kind: "ok", market };
}
