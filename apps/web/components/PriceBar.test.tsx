import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import PriceBar from "./PriceBar";

describe("PriceBar", () => {
  it("renders in-range percentages", () => {
    render(<PriceBar yesPrice={0.42} noPrice={0.58} />);

    const bars = screen.getAllByRole("progressbar");
    expect(bars).toHaveLength(2);
    expect(bars[0]).toHaveAttribute("aria-valuenow", "42");
    expect(bars[0]).toHaveAttribute("aria-label", "Yes 42%, No 58%");
    expect(bars[1]).toHaveAttribute("aria-valuenow", "58");
    expect(bars[1]).toHaveAttribute("aria-label", "No 58%");
  });

  it("clamps out-of-range percentages into [0, 100]", () => {
    const { rerender } = render(<PriceBar yesPrice={1.5} noPrice={-0.2} />);

    let bars = screen.getAllByRole("progressbar");
    expect(bars[0]).toHaveAttribute("aria-valuenow", "100");
    expect(bars[1]).toHaveAttribute("aria-valuenow", "0");

    rerender(<PriceBar yesPrice={-0.5} noPrice={2} />);
    bars = screen.getAllByRole("progressbar");
    expect(bars[0]).toHaveAttribute("aria-valuenow", "0");
    expect(bars[1]).toHaveAttribute("aria-valuenow", "100");
  });

  it("falls back to 0 for NaN and non-finite input", () => {
    render(<PriceBar yesPrice={NaN} noPrice={Infinity} />);

    const bars = screen.getAllByRole("progressbar");
    expect(bars[0]).toHaveAttribute("aria-valuenow", "0");
    expect(bars[1]).toHaveAttribute("aria-valuenow", "0");
  });

  it("omits the NO side when noPrice is not provided", () => {
    render(<PriceBar yesPrice={0.3} />);

    const bars = screen.getAllByRole("progressbar");
    expect(bars).toHaveLength(1);
    expect(bars[0]).toHaveAttribute("aria-label", "Yes 30%");
  });
});
