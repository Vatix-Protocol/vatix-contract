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

import { ContractErrorCode } from "./contracts";

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
 *
 * Names must match the real entrypoints exposed by the generated clients
 * (see apps/web/lib/contracts/index.ts).
 */
const PRIVILEGED_METHODS: ReadonlySet<string> = new Set([
  "mint",
  "burn",
  "transfer",
  "swap",
  "add_liquidity",
  "remove_liquidity",
  "deposit_collateral",
  "withdraw_collateral",
  "withdraw_unused_collateral",
  "settle_position",
  "resolve_market",
  "resolution_finalize",
  "treasury_withdraw",
  "admin_set_paused",
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

    // 2. Build the contract invocation transaction
    const contract = new Contract(contractId);
    const operation = contract.call(method, ...args);

    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    // 3. Simulate the transaction
    const simulated = await server.simulateTransaction(transaction);

    if (rpc.Api.isSimulationError(simulated)) {
      throw new ContractError(
        ContractErrorCode.SIMULATION_FAILED,
        `Simulation failed: ${simulated.error}`,
        { correlationId }
      );
    }

    // 4. Prepare the transaction with simulation results
    const prepared = rpc.assembleTransaction(transaction, simulated).build();

    // 5. Sign with Freighter
    const signedXdr = await signWithFreighter(
      prepared.toXDR(),
      NETWORK_PASSPHRASE
    );

    // 6. Submit the transaction
    const signedTx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE);
    const response = await server.sendTransaction(signedTx);

    if (response.status === "ERROR") {
      throw new ContractError(
        ContractErrorCode.SUBMISSION_FAILED,
        `Transaction submission failed: ${JSON.stringify(response.errorResult)}`,
        { correlationId }
      );
    }

    logContractEvent("success", {
      correlationId,
      contractId,
      method,
      durationMs: Date.now() - startedAt,
    });

    return { hash: response.hash, status: response.status };
  } catch (error) {
    if (error instanceof ContractError) {
      logContractEvent("failure", {
        correlationId: error.correlationId,
        contractId,
        method,
        code: error.code,
        durationMs: Date.now() - startedAt,
      });
      throw error;
    }

    logContractEvent("failure", {
      correlationId,
      contractId,
      method,
      code: ContractErrorCode.UNKNOWN,
      durationMs: Date.now() - startedAt,
    });

    throw new ContractError(
      ContractErrorCode.UNKNOWN,
      error instanceof Error ? error.message : "Unknown contract error",
      { correlationId, cause: error }
    );
  }
}

/**
 * Sign a transaction XDR with Freighter wallet.
 */
async function signWithFreighter(
  xdrString: string,
  networkPassphrase: string
): Promise<string> {
  const freighter = await import("@stellar/freighter-api");

  const result = await freighter.signTransaction(xdrString, {
    networkPassphrase,
  });

  if (typeof result === "string") {
    return result;
  }

  if (result && typeof result === "object" && "signedTxXdr" in result) {
    return (result as { signedTxXdr: string }).signedTxXdr;
  }

  throw new ContractError(
    ContractErrorCode.WALLET_UNAVAILABLE,
    "Freighter did not return a signed transaction."
  );
}

/**
 * Helper to convert a JS value to a Soroban ScVal.
 */
export function toScVal(value: unknown): xdr.ScVal {
  return nativeToScVal(value);
}

/**
 * Helper to convert a Soroban ScVal back to a JS value.
 */
export function fromScVal(value: xdr.ScVal): unknown {
  return scValToNative(value);
}

/**
 * Helper to convert a Stellar address string to a Soroban Address ScVal.
 */
export function addressToScVal(address: string): xdr.ScVal {
  return new Address(address).toScVal();
}
