/**
 * Shared stroops <-> display-unit conversion helpers.
 *
 * Soroban/Stellar assets use 7 decimal places: 1 display unit (token) is
 * represented on-chain as 10_000_000 stroops. This constant and the helpers
 * below are the single source of truth for that conversion so components
 * (e.g. PositionPanel, WithdrawForm) don't each define their own copy and
 * risk disagreeing (#1105).
 */
export const STROOPS_PER_UNIT = 10_000_000n;

/**
 * Converts a user-entered decimal amount string (e.g. "123.45") into
 * stroops as a bigint, without ever routing the value through `Number`.
 * This avoids precision loss for amounts near/above Number.MAX_SAFE_INTEGER,
 * which matters because on-chain amounts are i128 (#1104).
 *
 * Throws if the input is not a valid non-negative decimal string.
 */
export function toStroops(amount: string): bigint {
  const trimmed = amount.trim();
  const match = /^(\d+)(?:\.(\d+))?$/.exec(trimmed);
  if (!match) {
    throw new Error(`Invalid amount: ${amount}`);
  }

  const [, whole, fraction = ""] = match;
  // Pad/truncate the fractional part to exactly 7 digits (stroops precision).
  const fractionPadded = (fraction + "0000000").slice(0, 7);

  return BigInt(whole) * STROOPS_PER_UNIT + BigInt(fractionPadded || "0");
}

/** Formats a stroops bigint as a display-unit decimal string/number. */
export function fromStroops(stroops: bigint): number {
  return Number(stroops) / Number(STROOPS_PER_UNIT);
}
