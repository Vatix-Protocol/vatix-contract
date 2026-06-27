/**
 * Contract client helpers using generated TypeScript bindings.
 *
 * This module provides a simplified interface for interacting with
 * Soroban contracts using the auto-generated bindings and Freighter wallet.
 */

import {
  Contract,
  SorobanRpc,
  TransactionBuilder,
  Networks,
  BASE_FEE,
  Account,
  Operation,
  xdr,
} from "@stellar/stellar-sdk";
import { signTransaction } from "@stellar/freighter-api";

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
const server = new SorobanRpc.Server(SOROBAN_RPC_URL);

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
  if (!contractId) {
    throw new Error(
      "Contract ID not configured. Set NEXT_PUBLIC_*_CONTRACT_ID in .env.local"
    );
  }

  try {
    // 1. Load account from Horizon to get sequence number
    const accountResponse = await fetch(
      `${HORIZON_URL}/accounts/${sourceAddress}`
    );
    if (!accountResponse.ok) {
      throw new Error("Failed to load account from Horizon");
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

    if (SorobanRpc.Api.isSimulationError(simulated)) {
      throw new Error(`Simulation failed: ${simulated.error}`);
    }

    if (!simulated.result) {
      throw new Error("Simulation returned no result");
    }

    // 5. Prepare the transaction with simulation results
    const prepared = SorobanRpc.assembleTransaction(
      transaction,
      simulated
    ).build();

    // 6. Sign with Freighter
    const signedResult = await signTransaction(prepared.toXDR(), {
      networkPassphrase: NETWORK_PASSPHRASE,
      address: sourceAddress,
    });

    if (signedResult.error) {
      throw new Error(`Freighter signing failed: ${signedResult.error}`);
    }

    // 7. Submit the signed transaction
    const signedTx = TransactionBuilder.fromXDR(
      signedResult.signedTxXdr,
      NETWORK_PASSPHRASE
    );

    const sendResponse = await server.sendTransaction(signedTx);

    if (sendResponse.status === "ERROR") {
      throw new Error(
        `Transaction submission failed: ${sendResponse.errorResult}`
      );
    }

    // 8. Wait for transaction confirmation (optional but recommended)
    let getResponse = await server.getTransaction(sendResponse.hash);
    let attempts = 0;
    const maxAttempts = 20;

    while (
      getResponse.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND &&
      attempts < maxAttempts
    ) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      getResponse = await server.getTransaction(sendResponse.hash);
      attempts++;
    }

    if (getResponse.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`Transaction failed: ${getResponse.resultXdr}`);
    }

    return {
      hash: sendResponse.hash,
      status: sendResponse.status,
    };
  } catch (error) {
    console.error("Contract invocation error:", error);
    throw error;
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
  if (!contractId) {
    throw new Error(
      "Contract ID not configured. Set NEXT_PUBLIC_*_CONTRACT_ID in .env.local"
    );
  }

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

    if (SorobanRpc.Api.isSimulationError(simulated)) {
      throw new Error(`Simulation failed: ${simulated.error}`);
    }

    if (!simulated.result) {
      throw new Error("Simulation returned no result");
    }

    // Decode the result
    const resultValue = simulated.result.retval;

    // Return the raw XDR value - caller should decode based on expected type
    return resultValue as unknown as T;
  } catch (error) {
    console.error("Contract query error:", error);
    throw error;
  }
}

/**
 * Helper to convert string amounts to i128 XDR values (Soroban amounts are i128)
 */
export function amountToScVal(amount: string | number): xdr.ScVal {
  const amountBigInt = BigInt(amount);
  return xdr.ScVal.scvI128(
    new xdr.Int128Parts({
      lo: xdr.Uint64.fromString((amountBigInt & BigInt("0xFFFFFFFFFFFFFFFF")).toString()),
      hi: xdr.Int64.fromString((amountBigInt >> BigInt(64)).toString()),
    })
  );
}

/**
 * Helper to convert addresses to ScVal
 */
export function addressToScVal(address: string): xdr.ScVal {
  return xdr.ScVal.scvAddress(xdr.ScAddress.scAddressTypeAccount(
    xdr.PublicKey.publicKeyTypeEd25519(
      Buffer.from(address, 'base64')
    )
  ));
}

/**
 * Helper to convert u32 to ScVal
 */
export function u32ToScVal(value: number): xdr.ScVal {
  return xdr.ScVal.scvU32(value);
}

/**
 * Helper to convert boolean to ScVal
 */
export function boolToScVal(value: boolean): xdr.ScVal {
  return xdr.ScVal.scvBool(value);
}

/**
 * Helper to convert string to ScVal
 */
export function stringToScVal(value: string): xdr.ScVal {
  return xdr.ScVal.scvString(value);
}
