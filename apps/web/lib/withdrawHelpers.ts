/**
 * Withdraw form helpers — pure utility functions for the withdraw form.
 *
 * All functions have no side effects and no React dependency so they can be
 * unit-tested independently and imported from a single canonical place.
 */

// ── Types ──────────────────────────────────────────────────────────────────

/** A token balance with decimal precision. */
export interface TokenBalance {
  /** Raw integer amount in the token's smallest unit (e.g. stroops for USDC). */
  raw: bigint;
  /** Number of decimal places for the token (e.g. 7 for USDC on Stellar). */
  decimals: number;
}

/** Validation result returned by `validateWithdrawAmount`. */
export interface WithdrawValidationResult {
  valid: boolean;
  /** Human-readable error message; undefined when valid. */
  error?: string;
}

// ── Helpers ────────────────────────────────────────────────────────────────

/**
 * Convert a display-unit string entered by the user into the raw integer
 * amount used by the contract (e.g. "1.5" → 15_000_000n for 7 decimals).
 *
 * Returns `null` when the input is not a valid positive number.
 */
export function parseWithdrawAmount(
  displayAmount: string,
  decimals: number,
): bigint | null {
  const trimmed = displayAmount.trim();
  if (trimmed === "" || isNaN(Number(trimmed))) return null;

  const parsed = parseFloat(trimmed);
  if (parsed <= 0 || !isFinite(parsed)) return null;

  return BigInt(Math.round(parsed * 10 ** decimals));
}

/**
 * Convert a raw integer amount back to a display string, trimming trailing zeros.
 *
 * @example
 * formatWithdrawAmount(15_000_000n, 7) // "1.5"
 * formatWithdrawAmount(10_000_000n, 7) // "1"
 */
export function formatWithdrawAmount(raw: bigint, decimals: number): string {
  const display = Number(raw) / 10 ** decimals;
  return display.toFixed(decimals).replace(/\.?0+$/, "");
}

/**
 * Validate a withdraw amount entered by the user against the available balance.
 *
 * Rules:
 *  1. Input must parse as a valid positive number.
 *  2. Amount must not exceed the available balance.
 */
export function validateWithdrawAmount(
  displayAmount: string,
  balance: TokenBalance,
): WithdrawValidationResult {
  const raw = parseWithdrawAmount(displayAmount, balance.decimals);

  if (raw === null) {
    return { valid: false, error: "Please enter a valid amount." };
  }

  if (raw > balance.raw) {
    return { valid: false, error: "Amount exceeds available balance." };
  }

  return { valid: true };
}

/**
 * Return the maximum withdrawable display amount from a balance as a string.
 * Convenience wrapper for the Max button in the withdraw form.
 */
export function maxWithdrawDisplay(balance: TokenBalance): string {
  return formatWithdrawAmount(balance.raw, balance.decimals);
}
