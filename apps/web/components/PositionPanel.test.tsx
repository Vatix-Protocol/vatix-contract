import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PositionPanel } from "./PositionPanel";

const { getPosition, wallet } = vi.hoisted(() => ({
  getPosition: vi.fn(),
  wallet: { address: null as string | null },
}));

vi.mock("@/context/WalletContext", () => ({ useWallet: () => ({ address: wallet.address }) }));
vi.mock("@/lib/contract-client", () => ({ getPosition }));
vi.mock("./DepositForm", () => ({ DepositForm: () => null }));
vi.mock("./WithdrawForm", () => ({ WithdrawForm: () => null }));

describe("PositionPanel live position", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wallet.address = "G_ALICE";
    getPosition.mockResolvedValue(null);
  });

  it("refetches get_position when the address or market changes and shows an empty state for null", async () => {
    const { rerender } = render(<PositionPanel marketId="7" />);

    await waitFor(() => expect(getPosition).toHaveBeenCalledWith(7, "G_ALICE"));
    expect(await screen.findByText("No open position")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Deposit funds to open a position in this market" }),
    ).toBeInTheDocument();

    wallet.address = "G_BOB";
    rerender(<PositionPanel marketId="7" />);
    await waitFor(() => expect(getPosition).toHaveBeenCalledWith(7, "G_BOB"));

    rerender(<PositionPanel marketId="9" />);
    await waitFor(() => expect(getPosition).toHaveBeenCalledWith(9, "G_BOB"));
    expect(getPosition).toHaveBeenCalledTimes(3);
  });
});
