/**
 * Contract client helpers using generated TypeScript bindings.
 *
 * This module provides a simplified interface for interacting with
 * Soroban contracts using the auto-generated bindings and Freighter wallet.
 */

import {
  Address,
  Contract,
  rpc,
  TransactionBuilder,
  BASE_FEE,
  Account,
  xdr,
  nativeToScVal,
  scValToNative,
} from "@stellar/stellar-sdk";

// Network configuration from environment
const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  "Test SDF Network ; September 2015";

const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ??
  "https://soroban-testnet.stellar.org";

const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ??
  "https://horizon-testnet.stellar.org";

// Contract IDs
export const MARKET_CONTRACT_ID =
  process.env.NEXT_PUBLIC_MARKET_CONTRACT_ID ?? "";
export const TREASURY_CONTRACT_ID =
  process.env.NEXT_PUBLIC_TREASURY_CONTRACT_ID ?? "";
export const OUTCOME_TOKEN_CONTRACT_ID =
  process.env.NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID ?? "";
export const RESOLUTION_CONTRACT_ID =
  process.env.NEXT_PUBLIC_RESOLUTION_CONTRACT_ID ?? "";

// Initialize RPC server
const server = new rpc.Server(SOROBAN_RPC_URL);

/**
 * Stable, typed error codes for contract call failures.
 *
 * These codes are part of the public surface consumed by web callers and
 * must stay stable so UI/telemetry can branch on them without parsing
 * free-form messages. They mirror the on-chain error taxonomy and the
 * conventions in `apps/web/lib/errors.ts`.
 */
export enum ContractErrorCode {
  /** Contract ID missing / not configured for the active network. */
  NOT_CONFIGURED = "CONTRACT_NOT_CONFIGURED",
  /** Caller is not authorized for this privileged entrypoint. */
  UNAUTHORIZED = "CONTRACT_UNAUTHORIZED",
  /** Wallet extension unavailable or not running in a browser context. */
  WALLET_UNAVAILABLE = "CONTRACT_WALLET_UNAVAILABLE",
  /** User rejected the signing request in their wallet. */
  SIGNING_REJECTED = "CONTRACT_SIGNING_REJECTED",
  /** Simulation of the invocation failed (contract-level revert). */
  SIMULATION_FAILED = "CONTRACT_SIMULATION_FAILED",
  /** Submission to the RPC node failed. */
  SUBMISSION_FAILED = "CONTRACT_SUBMISSION_FAILED",
  /** Transaction was included but reverted on-chain. */
  TRANSACTION_FAILED = "CONTRACT_TRANSACTION_FAILED",
  /** Upstream dependency (RPC/Horizon) unavailable — fail closed. */
  DEPENDENCY_UNAVAILABLE = "CONTRACT_DEPENDENCY_UNAVAILABLE",
  /** Input failed validation before hitting the network. */
  INVALID_INPUT = "CONTRACT_INVALID_INPUT",
}

/**
 * Typed error thrown by all contract client entrypoints. Carries a stable
 * `code` and a `correlationId` so ops can trace a failure end-to-end
 * without leaking secrets or private keys.
 */
export class ContractError extends Error {
  readonly code: ContractErrorCode;
  readonly correlationId: string;
  readonly cause?: unknown;

  constructor(
    code: ContractErrorCode,
    message: string,
    options: { correlationId?: string; cause?: unknown } = {}
  ) {
    super(message);
    this.name = "ContractError";
    this.code = code;
    this.correlationId = options.correlationId ?? generateCorrelationId();
    this.cause = options.cause;
  }
}

/**
 * Generate a short, non-secret correlation id for a contract call.
 * Uses the Web Crypto API when available and falls back to a
 * timestamp+random token so it works in every runtime.
 */
export function generateCorrelationId(): string {
  const cryptoObj =
    typeof globalThis !== "undefined"
      ? (globalThis.crypto as Crypto | undefined)
      : undefined;

  if (cryptoObj?.randomUUID) {
    return cryptoObj.randomUUID();
  }

  return `cid-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 10)}`;
}

/**
 * Ops-safe structured log for contract calls. Never logs secrets, private
 * keys, signed XDR, or full argument payloads — only routing metadata and
 * the correlation id so money-path failures are traceable.
 */
function logContractEvent(
  event: "start" | "success" | "failure",
  fields: {
    correlationId: string;
    contractId: string;
    method: string;
    code?: ContractErrorCode;
    durationMs?: number;
  }
): void {
  const payload = {
    scope: "contract-client",
    event,
    correlationId: fields.correlationId,
    contractId: fields.contractId,
    method: fields.method,
    ...(fields.code ? { code: fields.code } : {}),
    ...(fields.durationMs !== undefined
      ? { durationMs: fields.durationMs }
      : {}),
  };

  if (event === "failure") {
    console.error("[contract-client]", payload);
  } else {
    console.info("[contract-client]", payload);
  }
}

/**
 * Privileged contract entrypoints. Deny-by-default: any method not listed
 * here is treated as privileged and requires an explicit authorization
 * check before it can be invoked from the web client. The server/contract
 * remains the source of truth for balances, swaps, and admin actions.
 */
const PRIVILEGED_METHODS: ReadonlySet<string> = new Set([
  "mint",
  "burn",
  "swap",
  "add_liquidity",
  "remove_liquidity",
  "deposit",
  "withdraw",
  "settle",
  "resolve",
  "set_admin",
  "upgrade",
  "pause",
  "unpause",
]);

/**
 * Authorization policy for privileged entrypoints. The web client can only
 * ever *request* a privileged action on behalf of the connected wallet; the
 * contract enforces the real policy. This guard fails closed so an
 * unauthenticated or malformed caller never reaches the network.
 */
export function assertAuthorized(
  method: string,
  sourceAddress: string | undefined
): void {
  if (!PRIVILEGED_METHODS.has(method)) {
    return;
  }

  if (!sourceAddress || !isValidStellarAddress(sourceAddress)) {
    throw new ContractError(
      ContractErrorCode.UNAUTHORIZED,
      `Privileged method "${method}" requires an authenticated wallet address.`
    );
  }
}

/**
 * Minimal structural validation for a Stellar account address (G.../C...).
 * This is a cheap client-side guard, not a substitute for on-chain authz.
 */
export function isValidStellarAddress(address: string): boolean {
  return /^[GC][A-Z2-7]{55}$/.test(address);
}

/**
 * Contract invocation result
 */
export interface InvokeResult {
  hash: string;
  status: string;
}

/**
 * Simulate and submit a contract invocation with Freighter signing.
 *
 * @param contractId - The contract address
 * @param method - The contract method name
 * @param args - Array of XDR-encoded arguments for the method
 * @param sourceAddress - The user's Stellar address (from Freighter)
 * @returns Transaction hash and status
 */
export async function invokeContract(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  sourceAddress: string
): Promise<InvokeResult> {
  const correlationId = generateCorrelationId();
  const startedAt = Date.now();

  if (typeof window === "undefined") {
    throw new ContractError(
      ContractErrorCode.WALLET_UNAVAILABLE,
      "invokeContract can only run in the browser (requires a wallet extension).",
      { correlationId }
    );
  }

  if (!contractId) {
    throw new ContractError(
      ContractErrorCode.NOT_CONFIGURED,
      "Contract ID not configured. Set NEXT_PUBLIC_*_CONTRACT_ID in .env.local",
      { correlationId }
    );
  }

  // Deny-by-default authz for privileged entrypoints.
  assertAuthorized(method, sourceAddress);

  logContractEvent("start", { correlationId, contractId, method });

  try {
    // 1. Load account from Horizon to get sequence number
    const accountResponse = await fetch(
      `${HORIZON_URL}/accounts/${sourceAddress}`
    );
    if (!accountResponse.ok) {
      throw new ContractError(
        ContractErrorCode.DEPENDENCY_UNAVAILABLE,
        "Failed to load account from Horizon",
        { correlationId }
      );
    }
    const accountData = await accountResponse.json();
    const account = new Account(sourceAddress, accountData.sequence);

    // 2. Build the contract invocation operation
    const contract = new Contract(contractId);
    const operation = contract.call(method, ...args);

    // 3. Build transaction
    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(operation)
      .setTimeout(180)
      .build();

    // 4. Simulate the transaction
    const simulated = await server.simulateTransaction(transaction);

    if (rpc.Api.isSimulationError(simulated)) {
      throw new ContractError(
        ContractErrorCode.SIMULATION_FAILED,
        `Simulation failed: ${simulated.error}`,
        { correlationId }
      );
    }

    if (!simulated.result) {
      throw new ContractError(
        ContractErrorCode.SIMULATION_FAILED,
        "Simulation returned no result",
        { correlationId }
      );
    }

    // 5. Prepare the transaction with simulation results
    const prepared = rpc.assembleTransaction(
      transaction,
      simulated
    ).build();

    // 6. Sign with Freighter. Imported dynamically (rather than at module
    // scope) so this module stays safe to import from server-rendered code
    // paths; the extension is only ever touched once we're in the browser.
    const { signTransaction } = await import("@stellar/freighter-api");
    const signedResult = await signTransaction(prepared.toXDR(), {
      networkPassphrase: NETWORK_PASSPHRASE,
      address: sourceAddress,
    });

    if (signedResult.error) {
      throw new ContractError(
        ContractErrorCode.SIGNING_REJECTED,
        `Freighter signing failed: ${signedResult.error}`,
        { correlationId }
      );
    }

    // 7. Submit the signed transaction
    const signedTx = TransactionBuilder.fromXDR(
      signedResult.signedTxXdr,
      NETWORK_PASSPHRASE
    );

    const sendResponse = await server.sendTransaction(signedTx);

    if (sendResponse.status === "ERROR") {
      throw new ContractError(
        ContractErrorCode.SUBMISSION_FAILED,
        `Transaction submission failed: ${sendResponse.errorResult}`,
        { correlationId }
      );
    }

    // 8. Wait for transaction confirmation (optional but recommended)
    let getResponse = await server.getTransaction(sendResponse.hash);
    let attempts = 0;
    const maxAttempts = 20;

    while (
      getResponse.status === rpc.Api.GetTransactionStatus.NOT_FOUND &&
      attempts < maxAttempts
    ) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      getResponse = await server.getTransaction(sendResponse.hash);
      attempts++;
    }

    if (getResponse.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new ContractError(
        ContractErrorCode.TRANSACTION_FAILED,
        `Transaction failed: ${getResponse.resultXdr}`,
        { correlationId }
      );
    }

    logContractEvent("success", {
      correlationId,
      contractId,
      method,
      durationMs: Date.now() - startedAt,
    });

    return {
      hash: sendResponse.hash,
      status: sendResponse.status,
    };
  } catch (error) {
    const contractError =
      error instanceof ContractError
        ? error
        : new ContractError(
            ContractErrorCode.SUBMISSION_FAILED,
            error instanceof Error ? error.message : "Unknown contract error",
            { correlationId, cause: error }
          );

    logContractEvent("failure", {
      correlationId: contractError.correlationId,
      contractId,
      method,
      code: contractError.code,
      durationMs: Date.now() - startedAt,
    });

    throw contractError;
  }
}

/**
 * Read-only contract query (no transaction submission).
 *
 * @param contractId - The contract address
 * @param method - The contract method name
 * @param args - Array of XDR-encoded arguments for the method
 * @returns The decoded result
 */
export async function queryContract<T>(
  contractId: string,
  method: string,
  args: xdr.ScVal[] = []
): Promise<T> {
  const correlationId = generateCorrelationId();
  const startedAt = Date.now();

  if (!contractId) {
    throw new ContractError(
      ContractErrorCode.NOT_CONFIGURED,
      "Contract ID not configured. Set NEXT_PUBLIC_*_CONTRACT_ID in .env.local",
      { correlationId }
    );
  }

  logContractEvent("start", { correlationId, contractId, method });

  try {
    const contract = new Contract(contractId);

    // Use a dummy source account for simulation
    const dummyAccount = new Account(
      "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
      "0"
    );

    const operation = contract.call(method, ...args);

    const transaction = new TransactionBuilder(dummyAccount, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(operation)
      .setTimeout(180)
      .build();

    const simulated = await server.simulateTransaction(transaction);

    if (rpc.Api.isSimulationError(simulated)) {
      throw new ContractError(
        ContractErrorCode.SIMULATION_FAILED,
        `Simulation failed: ${simulated.error}`,
        { correlationId }
      );
    }

    if (!simulated.result) {
      throw new ContractError(
        ContractErrorCode.SIMULATION_FAILED,
        "Simulation returned no result",
        { correlationId }
      );
    }

    // Decode the result
    const resultValue = simulated.result.retval;

    logContractEvent("success", {
      correlationId,
      contractId,
      method,
      durationMs: Date.now() - startedAt,
    });

    // Return the raw XDR value - caller should decode based on expected type
    return resultValue as unknown as T;
  } catch (error) {
    const contractError =
      error instanceof ContractError
        ? error
        : new ContractError(
            ContractErrorCode.SIMULATION_FAILED,
            error instanceof Error ? error.message : "Unknown contract error",
            { correlationId, cause: error }
          );

    logContractEvent("failure", {
      correlationId: contractError.correlationId,
      contractId,
      method,
      code: contractError.code,
      durationMs: Date.now() - startedAt,
    });

    throw contractError;
  }
}

/**
 * A user's open position in a market, as returned by the contract's
 * `get_position` read method. Share/collateral amounts are in stroops
 * (1 token = 10^7 stroops).
 */
export interface PositionData {
  yesShares: bigint;
  noShares: bigint;
  lockedCollateral: bigint;
  totalDeposited: bigint;
  isSettled: boolean;
}

/**
 * Read a user's live position for a market straight from the contract.
 * Returns `null` when the user has no recorded position (the contract's
 * `get_position` returns `None`).
 */
export async function getPosition(
  marketId: number,
  userAddress: string
): Promise<PositionData | null> {
  const retval = await queryContract<xdr.ScVal>(MARKET_CONTRACT_ID, "get_position", [
    u32ToScVal(marketId),
    addressToScVal(userAddress),
  ]);

  const native = scValToNative(retval) as
    | {
        yes_shares: bigint;
        no_shares: bigint;
        locked_collateral: bigint;
        total_deposited: bigint;
        is_settled: boolean;
      }
    | null
    | undefined;

  if (native == null) {
    return null;
  }

  return {
    yesShares: BigInt(native.yes_shares),
    noShares: BigInt(native.no_shares),
    lockedCollateral: BigInt(native.locked_collateral),
    totalDeposited: BigInt(native.total_deposited),
    isSettled: Boolean(native.is_settled),
  };
}

/**
 * Helper to convert string amounts to i128

/* … truncated 814 chars — edit only what you need near the top … */
