import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Market } from "@/lib/markets";
import { MarketCard } from "./MarketCard";

const market: Market = {
  id: "1", question: "Will it rain?", status: "open", yesPrice: 0.4,
  noPrice: 0.6, volume: "100", endsAt: "2026-12-01", closedToDeposits: false,
};

describe("MarketCard", () => {
  it("preserves the valid end-time label", () => {
    render(<MarketCard market={market} />);
    expect(screen.getByText("ends 2026-12-01")).toBeInTheDocument();
  });

  it.each([undefined, null, "", "not-a-date", "Invalid Date"])(
    "renders a fallback for end time %s", (endsAt) => {
      render(<MarketCard market={{ ...market, endsAt } as Market} />);
      expect(screen.getByText("ends Unknown")).toBeInTheDocument();
      expect(screen.getByText(market.question)).toBeInTheDocument();
    },
  );
});
