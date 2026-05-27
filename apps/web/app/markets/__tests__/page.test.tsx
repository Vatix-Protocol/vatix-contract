import { render } from "@testing-library/react";
import MarketsPage from "../page";

describe("MarketsPage", () => {
  it("renders without throwing", () => {
    const { container } = render(<MarketsPage />);
    expect(container).toBeInTheDocument();
  });
});
