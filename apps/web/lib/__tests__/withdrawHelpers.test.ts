import {
  parseWithdrawAmount,
  formatWithdrawAmount,
  validateWithdrawAmount,
  maxWithdrawDisplay,
} from "../withdrawHelpers";
import type { TokenBalance } from "../withdrawHelpers";

const xlmBalance: TokenBalance = { raw: 100_000_000n, decimals: 7 }; // 10 USDC

describe("parseWithdrawAmount", () => {
  test("parses valid decimal string", () => {
    expect(parseWithdrawAmount("1.5", 7)).toBe(15_000_000n);
  });
  test("parses whole number string", () => {
    expect(parseWithdrawAmount("10", 7)).toBe(100_000_000n);
  });
  test("returns null for empty string", () => {
    expect(parseWithdrawAmount("", 7)).toBeNull();
  });
  test("returns null for non-numeric input", () => {
    expect(parseWithdrawAmount("abc", 7)).toBeNull();
  });
  test("returns null for zero", () => {
    expect(parseWithdrawAmount("0", 7)).toBeNull();
  });
  test("returns null for negative number", () => {
    expect(parseWithdrawAmount("-1", 7)).toBeNull();
  });
  test("returns null for whitespace only", () => {
    expect(parseWithdrawAmount("   ", 7)).toBeNull();
  });
});

describe("formatWithdrawAmount", () => {
  test("formats 1 USDC correctly", () => {
    expect(formatWithdrawAmount(10_000_000n, 7)).toBe("1");
  });
  test("formats 1.5 USDC correctly", () => {
    expect(formatWithdrawAmount(15_000_000n, 7)).toBe("1.5");
  });
  test("trims trailing zeros", () => {
    expect(formatWithdrawAmount(10_000_000n, 7)).not.toContain(".0");
  });
  test("formats zero", () => {
    expect(formatWithdrawAmount(0n, 7)).toBe("0");
  });
});

describe("validateWithdrawAmount", () => {
  test("valid amount within balance passes", () => {
    expect(validateWithdrawAmount("5", xlmBalance).valid).toBe(true);
  });
  test("exact balance is valid", () => {
    expect(validateWithdrawAmount("10", xlmBalance).valid).toBe(true);
  });
  test("amount exceeding balance is invalid", () => {
    const r = validateWithdrawAmount("11", xlmBalance);
    expect(r.valid).toBe(false);
    expect(r.error).toMatch(/exceed/i);
  });
  test("non-numeric input is invalid", () => {
    expect(validateWithdrawAmount("abc", xlmBalance).valid).toBe(false);
  });
  test("empty string is invalid", () => {
    expect(validateWithdrawAmount("", xlmBalance).valid).toBe(false);
  });
});

describe("maxWithdrawDisplay", () => {
  test("returns correct display amount for balance", () => {
    expect(maxWithdrawDisplay(xlmBalance)).toBe("10");
  });
  test("handles zero balance", () => {
    expect(maxWithdrawDisplay({ raw: 0n, decimals: 7 })).toBe("0");
  });
});
