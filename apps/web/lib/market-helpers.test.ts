// @vitest-environment node
// Unit tests for the market detail lookup helpers.
//
// Run with: `pnpm --filter web run test`
import { describe, expect, test, vi } from "vitest";
import { findMarketById, isValidMarketId } from "./market-helpers";
import type { Market } from "./markets";

const markets: Market[] = [
  {
    id: "1",
    question: "Will it rain?",
    yesPrice: 0.6,
    noPrice: 0.4,
    volume: "100",
    status: "open",
    endsAt: "2026-01-01",
    closedToDeposits: false,
  },
  {
    id: "0",
    question: "Genesis market",
    yesPrice: 0.5,
    noPrice: 0.5,
    volume: "0",
    status: "open",
    endsAt: "2026-01-01",
    closedToDeposits: false,
  },
];

vi.mock("./markets", () => ({
  getMarkets: async () => markets,
}));

describe("isValidMarketId", () => {
  test("accepts canonical ids", () => {
    expect(isValidMarketId("1")).toBe(true);
    expect(isValidMarketId("0")).toBe(true);
    expect(isValidMarketId("123")).toBe(true);
  });

  test("rejects malformed ids", () => {
    expect(isValidMarketId("")).toBe(false);
    expect(isValidMarketId("abc")).toBe(false);
    expect(isValidMarketId("007")).toBe(false);
    expect(isValidMarketId("7 ")).toBe(false);
    expect(isValidMarketId(" 7")).toBe(false);
    expect(isValidMarketId("-1")).toBe(false);
  });
});

describe("findMarketById", () => {
  test("returns the matching market for a valid id", async () => {
    const result = await findMarketById("1");
    expect(result.kind).toBe("ok");
    if (result.kind === "ok") {
      expect(result.market.id).toBe("1");
    }
  });

  test("returns not-found for a well-formed id with no match", async () => {
    expect(await findMarketById("999")).toEqual({
      kind: "not-found",
      id: "999",
    });
  });

  test("returns invalid-id without scanning for malformed ids", async () => {
    for (const bad of ["abc", "", "007", "7 "]) {
      expect(await findMarketById(bad)).toEqual({ kind: "invalid-id", id: bad });
    }
  });
});
