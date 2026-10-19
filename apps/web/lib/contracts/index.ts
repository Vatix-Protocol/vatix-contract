/**
 * Contract bindings index.
 *
 * Re-exports the generated Soroban contract clients and the environment
 * configuration used by web callers. Keep this surface in sync with the
 * current `vatix-contract` entrypoints — see README.md "Sync Invariants".
 */

export * from './market';
export * from './treasury';
export * from './outcome-token';
export * from './resolution';

/**
 * Stable error codes for contract call failures.
 *
 * This is the single source of truth for contract error codes in the web
 * package. `apps/web/lib/contract-client.ts` imports this object rather than
 * declaring its own taxonomy. Aligned with `apps/web/lib/errors.ts`. Do not
 * introduce ad-hoc string errors in callers — map every failure to one of
 * these codes so the web layer cannot drift from on-chain error semantics.
 */
export const ContractErrorCode = {
  /** Caller is not authorized for a privileged entrypoint. */
  UNAUTHORIZED: 'CONTRACT_UNAUTHORIZED',
  /** Requested entrypoint does not exist on the contract. */
  UNKNOWN_ENTRYPOINT: 'CONTRACT_UNKNOWN_ENTRYPOINT',
  /** Arguments failed client-side validation before submission. */
  INVALID_ARGUMENT: 'CONTRACT_INVALID_ARGUMENT',
  /** RPC/DB/Redis dependency unavailable — write paths fail closed. */
  DEPENDENCY_UNAVAILABLE: 'CONTRACT_DEPENDENCY_UNAVAILABLE',
  /** Replayed or concurrent request rejected by idempotency guard. */
  DUPLICATE_REQUEST: 'CONTRACT_DUPLICATE_REQUEST',
  /** Network/address drift between testnet and mainnet. */
  NETWORK_MISMATCH: 'CONTRACT_NETWORK_MISMATCH',
  /** Contract returned a domain error (insufficient balance, closed market). */
  CONTRACT_REJECTED: 'CONTRACT_REJECTED',
  /** Unclassified failure — treat as fail-closed. */
  UNKNOWN: 'CONTRACT_UNKNOWN',
  /** Client is missing required contract/RPC configuration. */
  NOT_CONFIGURED: 'CONTRACT_NOT_CONFIGURED',
  /** Wallet provider is unavailable or not connected. */
  WALLET_UNAVAILABLE: 'CONTRACT_WALLET_UNAVAILABLE',
  /** User rejected the signing request in their wallet. */
  SIGNING_REJECTED: 'CONTRACT_SIGNING_REJECTED',
  /** Transaction simulation failed before submission. */
  SIMULATION_FAILED: 'CONTRACT_SIMULATION_FAILED',
  /** Transaction submission to the network failed. */
  SUBMISSION_FAILED: 'CONTRACT_SUBMISSION_FAILED',
  /** Submitted transaction failed on-chain. */
  TRANSACTION_FAILED: 'CONTRACT_TRANSACTION_FAILED',
  /** Input failed client-side validation (alias of INVALID_ARGUMENT). */
  INVALID_INPUT: 'CONTRACT_INVALID_INPUT',
} as const;

export type ContractErrorCode =
  (typeof ContractErrorCode)[keyof typeof ContractErrorCode];

/**
 * Typed contract call failure carrying a stable error code and a correlation
 * id so failures are traceable across web, RPC, and contract logs.
 */
export class ContractCallError extends Error {
  readonly code: ContractErrorCode;
  readonly correlationId: string;

  constructor(code: ContractErrorCode, correlationId: string, message?: string) {
    super(message ?? code);
    this.name = 'ContractCallError';
    this.code = code;
    this.correlationId = correlationId;
  }
}

/**
 * Privileged entrypoints that require an explicit role check before
 * invocation. Deny-by-default: anything not listed here is treated as
 * unprivileged, and anything listed here must pass `assertAuthorized`.
 *
 * Names must match the generated client functions exported from
 * `./market`, `./treasury`, and `./resolution` — see
 * `apps/web/lib/contracts/index.test.ts` which asserts this invariant.
 */
export const PRIVILEGED_ENTRYPOINTS = [
  'admin_set_paused',
  'admin_set_fee_bps',
  'admin_set_treasury',
  'treasury_withdraw',
  'resolution_finalize',
  'resolve_market',
] as const;

export type PrivilegedEntrypoint = (typeof PRIVILEGED_ENTRYPOINTS)[number];

/**
 * Money-path entrypoints that must carry a correlation id and emit
 * ops-safe metrics. Secrets and private keys are never logged.
 *
 * Names must match the generated client functions exported from
 * `./market`, `./outcome-token`, and `./resolution` — see
 * `apps/web/lib/contracts/index.test.ts` which asserts this invariant.
 */
export const MONEY_PATH_ENTRYPOINTS = [
  'deposit_collateral',
  'withdraw_unused_collateral',
  'settle_position',
  'resolve_market',
  'mint',
  'burn',
  'transfer',
] as const;

export type MoneyPathEntrypoint = (typeof MONEY_PATH_ENTRYPOINTS)[number];

/**
 * Deny-by-default authz guard for privileged entrypoints.
 *
 * The server/contract remains the source of truth for balances, swaps, and
 * admin; this guard prevents untrusted clients from bypassing policy on the
 * web side. Throws a typed `ContractCallError` on failure.
 */
export function assertAuthorized(
  entrypoint: string,
  role: string | null | undefined,
  correlationId: string,
): void {
  if (!(PRIVILEGED_ENTRYPOINTS as readonly string[]).includes(entrypoint)) {
    return;
  }
  if (role !== 'admin') {
    throw new ContractCallError(
      ContractErrorCode.UNAUTHORIZED,
      correlationId,
      `unauthorized entrypoint: ${entrypoint}`,
    );
  }
}

/**
 * Ops-safe metrics/logging hook for money-path calls.
 *
 * Emits method, contract id, correlation id, and outcome only. Never logs
 * secrets, private keys, or full payloads.
 */
export function recordMoneyPathCall(params: {
  entrypoint: string;
  contractId: string;
  correlationId: string;
  outcome: 'success' | 'failure';
  errorCode?: ContractErrorCode;
}): void {
  if (!(MONEY_PATH_ENTRYPOINTS as readonly string[]).includes(params.entrypoint)) {
    return;
  }
  // Structured, secret-free log line. Wire to the metrics sink in ops config.
  // eslint-disable-next-line no-console
  console.info('[contract]', {
    entrypoint: params.entrypoint,
    contractId: params.contractId,
    correlationId: params.correlationId,
    outcome: params.outcome,
    errorCode: params.errorCode,
  });
}

/**
 * Generate a correlation id for a contract call. Callers must pass this
 * through to `assertAuthorized` and `recordMoneyPathCall` so failures are
 * traceable and replayed requests can be deduplicated.
 */
export function newCorrelationId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `cid-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
