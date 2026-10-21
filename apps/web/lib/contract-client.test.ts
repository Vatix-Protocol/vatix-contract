import { afterEach, describe, expect, it, vi } from "vitest";
import { Address, rpc, xdr } from "@stellar/stellar-sdk";
import { confirmContractTransaction } from "./contract-client";

// Generated bindings are unnecessary for RPC confirmation and are produced
// separately by build:bindings.
vi.mock("./contracts", () => ({
  ContractErrorCode: {
    TRANSACTION_FAILED: "CONTRACT_TRANSACTION_FAILED",
    DEPENDENCY_UNAVAILABLE: "CONTRACT_DEPENDENCY_UNAVAILABLE",
  },
}));

afterEach(() => vi.restoreAllMocks());

describe("confirmed transaction events", () => {
  it("returns committed contract topics with their emitting address", async () => {
    const id = Buffer.alloc(32, 1);
    const topic = xdr.ScVal.scvSymbol("fee_retained_no_treasury");
    const event = new xdr.ContractEvent({
      ext: new xdr.ExtensionPoint(0), contractId: Address.contract(id).toScAddress().contractId(),
      type: xdr.ContractEventType.contract(),
      body: new xdr.ContractEventBody(0, new xdr.ContractEventV0({ topics: [topic], data: xdr.ScVal.scvVoid() })),
    });
    const poll = vi.spyOn(rpc.Server.prototype, "pollTransaction").mockResolvedValue({
      status: rpc.Api.GetTransactionStatus.SUCCESS,
      events: { contractEventsXdr: [[event]], transactionEventsXdr: [] },
    } as unknown as rpc.Api.GetTransactionResponse);
    const result = await confirmContractTransaction("hash");
    expect(poll).toHaveBeenCalledWith("hash", { attempts: 30 });
    expect(result).toEqual({ hash: "hash", status: "SUCCESS", events: [{ contractId: Address.contract(id).toString(), topics: [topic] }] });
  });

  it.each([
    [rpc.Api.GetTransactionStatus.FAILED, "CONTRACT_TRANSACTION_FAILED"],
    [rpc.Api.GetTransactionStatus.NOT_FOUND, "CONTRACT_DEPENDENCY_UNAVAILABLE"],
  ])("does not report success for %s", async (status, code) => {
    vi.spyOn(rpc.Server.prototype, "pollTransaction").mockResolvedValue({ status } as rpc.Api.GetTransactionResponse);
    await expect(confirmContractTransaction("hash")).rejects.toMatchObject({ code });
  });
});
