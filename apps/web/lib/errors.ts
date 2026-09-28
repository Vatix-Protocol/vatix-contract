/**
 * Turns a thrown contract-invocation error into a short, user-facing
 * revert reason for display in a toast/inline alert.
 */

/**
 * Numeric discriminant -> human-readable copy for `vatix-market-contract`'s
 * `ContractError` enum (see `contracts/market/src/error.rs`). Keep this in
 * sync with that enum's discriminants; the numbers are stable ABI so a drift
 * here just means a worse error message, not a broken build.
 */
export const MARKET_ERROR_MESSAGES: Record<number, string> = {
  // Market errors (1-9)
  1: "This market doesn't exist.",
  2: "This market has already been resolved.",
  3: "This market hasn't been resolved yet.",
  4: "This market has expired and is no longer accepting trades.",
  5: "This market isn't active right now.",
  6: "Deposits are currently closed for this market.",
  7: "You need to wait for the withdrawal cooldown to end before withdrawing again.",

  // Position errors (10-19)
  10: "You don't have enough collateral deposited to do that.",
  11: "This position has already been settled.",
  12: "You don't have a position in this market.",
  13: "That share amount isn't valid.",
  14: "Too many positions in that batch. Try a smaller batch.",

  // Oracle errors (20-29)
  20: "Oracle signature verification failed.",
  21: "This address isn't authorized to resolve this market.",
  22: "That resolution outcome isn't valid.",
  23: "The price oracle didn't return a price for this asset. Try again shortly.",

  // Validation errors (30-39)
  30: "That price is out of range.",
  31: "That quantity isn't valid.",
  32: "That timestamp isn't valid.",
  33: "That question isn't valid.",
  34: "Markets must have exactly two outcomes.",
  35: "That admin address isn't valid.",
  36: "That deposit is below the minimum allowed amount.",
  37: "That metadata URI is too long.",
  38: "That fee rate isn't valid.",

  // Authorization errors (40-49)
  40: "You're not authorized to do that.",
  41: "Only the contract admin can do that.",
  42: "This contract has already been initialized.",
  43: "There's no pending admin nomination to accept.",
  44: "There's no pending renounce request to confirm.",
  45: "An admin renounce request is already pending.",
  46: "That fee rate exceeds the maximum allowed.",

  // Token errors (50-59)
  50: "The token transfer failed. Check your balance and try again.",

  // Arithmetic errors (60-69)
  60: "That operation would overflow — try a smaller amount.",

  // Upgrade errors (70-79)
  70: "This contract needs a migration before it can be used again.",

  // Resolution errors (80-89)
  80: "The resolution for this market hasn't been finalized yet.",

  // Pause / initialization errors (90-99)
  90: "This contract hasn't been initialized yet.",
  91: "This contract is paused for maintenance. Please try again later.",

  // Security errors (100-109)
  100: "That action was blocked as a potential reentrant call. Please try again.",

  // Timelock errors (110-119)
  110: "There's no pending fee rate change to execute.",
  111: "The fee rate change timelock hasn't elapsed yet.",
};

/**
 * Looks up a Soroban contract error discriminant against the known market
 * contract error codes, returning `undefined` for unrecognized codes so
 * callers can fall back to a generic message.
 */
export function marketErrorMessage(code: number): string | undefined {
  return MARKET_ERROR_MESSAGES[code];
}

/**
 * Stable error codes for the Freighter wallet integration path. These are
 * surfaced to callers (and logged) instead of raw wallet/RPC messages so that
 * network-mismatch and configuration failures are actionable and never leak
 * secrets. See `docs/freighter-integration-guide.md`.
 */
export const FREIGHTER_ERROR_CODES = {
  /** The connected Freighter network does not match the expected passphrase. */
  NETWORK_MISMATCH: "FREIGHTER_NETWORK_MISMATCH",
  /** No expected network passphrase was configured for this environment. */
  MISSING_NETWORK_PASSPHRASE: "FREIGHTER_MISSING_NETWORK_PASSPHRASE",
  /** Freighter did not report a network passphrase for the active connection. */
  MISSING_WALLET_PASSPHRASE: "FREIGHTER_MISSING_WALLET_PASSPHRASE",
  /** The wallet connection attempt was rejected or failed. */
  CONNECT_FAILED: "FREIGHTER_CONNECT_FAILED",
} as const;

export type FreighterErrorCode =
  (typeof FREIGHTER_ERROR_CODES)[keyof typeof FREIGHTER_ERROR_CODES];

/**
 * Error thrown by the Freighter integration when a network/passphrase invariant
 * is violated. Carries a stable `code` so callers can branch on it and so logs
 * stay free of secrets (only the code and a correlation id are emitted).
 */
export class FreighterError extends Error {
  readonly code: FreighterErrorCode;
  readonly correlationId?: string;

  constructor(code: FreighterErrorCode, message: string, correlationId?: string) {
    super(message);
    this.name = "FreighterError";
    this.code = code;
    this.correlationId = correlationId;
  }
}

/**
 * Human-readable copy for each Freighter error code, safe to show in the UI.
 */
export const FREIGHTER_ERROR_MESSAGES: Record<FreighterErrorCode, string> = {
  [FREIGHTER_ERROR_CODES.NETWORK_MISMATCH]:
    "Your Freighter wallet is connected to a different network than this app expects. Switch networks in Freighter and try again.",
  [FREIGHTER_ERROR_CODES.MISSING_NETWORK_PASSPHRASE]:
    "This app isn't configured with an expected Stellar network. Contact support.",
  [FREIGHTER_ERROR_CODES.MISSING_WALLET_PASSPHRASE]:
    "Freighter didn't report a network passphrase. Reconnect your wallet and try again.",
  [FREIGHTER_ERROR_CODES.CONNECT_FAILED]:
    "Couldn't connect to Freighter. Make sure the extension is installed and unlocked.",
};

/**
 * Resolves the expected Stellar network passphrase for the current environment.
 * Fails closed: returns `undefined` when no passphrase is configured so callers
 * can reject signing rather than defaulting to an unsafe network.
 */
export function resolveExpectedNetworkPassphrase(
  configured?: string | null,
): string | undefined {
  const value = configured?.trim();
  return value ? value : undefined;
}

/**
 * Validates that the passphrase reported by the connected Freighter wallet
 * matches the expected network passphrase. Deny-by-default: any missing or
 * mismatched passphrase throws a `FreighterError` so signing/transactions are
 * rejected before they reach the network.
 */
export function assertFreighterNetwork(
  walletPassphrase: string | null | undefined,
  expectedPassphrase: string | null | undefined,
  correlationId?: string,
): void {
  const expected = resolveExpectedNetworkPassphrase(expectedPassphrase);
  if (!expected) {
    throw new FreighterError(
      FREIGHTER_ERROR_CODES.MISSING_NETWORK_PASSPHRASE,
      FREIGHTER_ERROR_MESSAGES[FREIGHTER_ERROR_CODES.MISSING_NETWORK_PASSPHRASE],
      correlationId,
    );
  }

  const actual = walletPassphrase?.trim();
  if (!actual) {
    throw new FreighterError(
      FREIGHTER_ERROR_CODES.MISSING_WALLET_PASSPHRASE,
      FREIGHTER_ERROR_MESSAGES[FREIGHTER_ERROR_CODES.MISSING_WALLET_PASSPHRASE],
      correlationId,
    );
  }

  if (actual !== expected) {
    throw new FreighterError(
      FREIGHTER_ERROR_CODES.NETWORK_MISMATCH,
      FREIGHTER_ERROR_MESSAGES[FREIGHTER_ERROR_CODES.NETWORK_MISMATCH],
      correlationId,
    );
  }
}

/**
 * Returns the user-facing message for a Freighter error code, falling back to
 * the generic connect-failure copy for unknown codes.
 */
export function freighterErrorMessage(code: string): string {
  return (
    FREIGHTER_ERROR_MESSAGES[code as FreighterErrorCode] ??
    FREIGHTER_ERROR_MESSAGES[FREIGHTER_ERROR_CODES.CONNECT_FAILED]
  );
}

export function parseContractError(err: unknown): string {
  if (err instanceof FreighterError) {
    return err.message;
  }

  const message = err instanceof Error ? err.message : String(err);

  // Soroban host trap, e.g. `Error(Contract, #10)` — surfaced as-is by the
  // SDK inside simulation/transaction failure messages.
  const contractError = message.match(/Error\(Contract,\s*#(\d+)\)/);
  if (contractError) {
    const code = Number(contractError[1]);
    const known = marketErrorMessage(code);
    if (known) {
      return known;
    }
    return `Contract rejected the transaction (error #${contractError[1]}).`;
  }

  const simulationFailed = message.match(/^Simulation failed:\s*([\s\S]+)$/);
  if (simulationFailed) {
    return `Transaction would fail: ${simulationFailed[1]}`;
  }

  const txFailed = message.match(/^Transaction failed:\s*([\s\S]+)$/);
  if (txFailed) {
    return `Transaction failed: ${txFailed[1]}`;
  }

  if (/denied|rejected|cancel/i.test(message)) {
    return "Request was rejected in the wallet.";
  }

  return message;
}
