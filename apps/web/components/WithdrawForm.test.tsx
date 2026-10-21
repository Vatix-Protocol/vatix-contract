import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { xdr } from "@stellar/stellar-sdk";
import { invokeContract, type InvocationEvent } from "@/lib/contract-client";
import { WithdrawForm } from "./WithdrawForm";

const { getPosition } = vi.hoisted(() => ({ getPosition: vi.fn() }));

vi.mock("@/context/WalletContext", () => ({ useWallet: () => ({ address: "user" }) }));
vi.mock("@/context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));
vi.mock("@/lib/contract-client", () => ({
  MARKET_CONTRACT_ID: "market",
  invokeContract: vi.fn(), getPosition,
  u32ToScVal: vi.fn(), addressToScVal: vi.fn(), amountToScVal: vi.fn(),
}));

const note = "A fee was retained by the protocol because no treasury is currently registered.";
const retained = { contractId: "market", topics: [xdr.ScVal.scvSymbol("fee_retained_no_treasury")] };

async function withdraw() {
  await waitFor(() => expect(getPosition).toHaveBeenCalled());
  fireEvent.change(screen.getByLabelText("Amount"), { target: { value: "1" } });
  fireEvent.click(screen.getByRole("button", { name: "Withdraw funds" }));
  await screen.findByRole("status");
}

describe("WithdrawForm fee transparency", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getPosition).mockResolvedValue({ totalDeposited: BigInt(100_000_000), lockedCollateral: BigInt(0) });
  });

  it.each([
    ["retained", [retained], true],
    ["routed", [{ contractId: "market", topics: [xdr.ScVal.scvSymbol("fee_collected")] }], false],
    ["another contract", [{ ...retained, contractId: "other" }], false],
    ["non-symbol topic", [{ contractId: "market", topics: [xdr.ScVal.scvU32(1)] }], false],
  ] as [string, InvocationEvent[], boolean][])("shows the note only for a market retained-fee event: %s", async (_, events, visible) => {
    vi.mocked(invokeContract).mockResolvedValue({ hash: "a".repeat(64), status: "SUCCESS", events: [...events] });
    render(<WithdrawForm />);
    await withdraw();
    if (visible) expect(screen.getByText(note)).toBeInTheDocument();
    else expect(screen.queryByText(note)).not.toBeInTheDocument();
  });

  it("clears a previous retained-fee note for the next withdrawal", async () => {
    vi.mocked(invokeContract)
      .mockResolvedValueOnce({ hash: "a".repeat(64), status: "SUCCESS", events: [retained] })
      .mockResolvedValueOnce({ hash: "b".repeat(64), status: "SUCCESS", events: [] });
    render(<WithdrawForm />);
    await withdraw();
    expect(screen.getByText(note)).toBeInTheDocument();
    await withdraw();
    expect(screen.queryByText(note)).not.toBeInTheDocument();
  });
});
