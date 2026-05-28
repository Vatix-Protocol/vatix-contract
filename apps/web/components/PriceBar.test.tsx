import { describe, it, expect } from "@jest/globals";
import { render } from "@testing-library/react";
import { PriceBar } from "./PriceBar";

describe("PriceBar", () => {
  it("renders without crashing", () => {
    const { container } = render(<PriceBar yesPrice={0.65} />);
    expect(container).toBeTruthy();
  });
});
