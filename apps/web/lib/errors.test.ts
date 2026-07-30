// Unit tests for the market contract error mapper.
//
// Run with: `node --test --import tsx apps/web/lib/errors.test.ts`
// (uses Node's built-in test runner; no extra test framework dependency).
import { test } from "node:test";
import assert from "node:assert/strict";
import { parseContractError, marketErrorMessage, MARKET_ERROR_MESSAGES } from "./errors";

test("maps a deposit failure (MarketClosedToDeposits, #6) to clear copy", () => {
  const err = new Error("Error(Contract, #6)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[6]);
});

test("maps a deposit failure (BelowMinDeposit, #36) to clear copy", () => {
  const err = new Error("Error(Contract, #36)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[36]);
});

test("maps a withdraw failure (WithdrawCooldownActive, #7) to clear copy", () => {
  const err = new Error("Error(Contract, #7)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[7]);
});

test("maps a withdraw failure (InsufficientCollateral, #10) to clear copy", () => {
  const err = new Error("Error(Contract, #10)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[10]);
});

test("maps a batch-settle failure (BatchTooLarge, #14) to clear copy", () => {
  const err = new Error("Error(Contract, #14)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[14]);
});

test("maps an auth failure (NotAdmin, #41) to clear copy", () => {
  const err = new Error("Error(Contract, #41)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[41]);
});

test("maps an auth failure (Unauthorized, #40) to clear copy", () => {
  const err = new Error("Error(Contract, #40)");
  assert.equal(parseContractError(err), MARKET_ERROR_MESSAGES[40]);
});

test("falls back to a generic message for an unmapped code", () => {
  const err = new Error("Error(Contract, #9999)");
  assert.equal(marketErrorMessage(9999), undefined);
  assert.equal(
    parseContractError(err),
    "Contract rejected the transaction (error #9999)."
  );
});

test("still surfaces simulation failures unrelated to a contract code", () => {
  const err = new Error("Simulation failed: insufficient fee");
  assert.equal(parseContractError(err), "Transaction would fail: insufficient fee");
});

test("still surfaces transaction failures unrelated to a contract code", () => {
  const err = new Error("Transaction failed: bad sequence number");
  assert.equal(parseContractError(err), "Transaction failed: bad sequence number");
});

test("still surfaces wallet rejections", () => {
  const err = new Error("User denied access");
  assert.equal(parseContractError(err), "Request was rejected in the wallet.");
});
