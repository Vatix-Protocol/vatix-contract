/**
 * Soroban RPC helpers using @stellar/stellar-sdk.
 *
 * All config comes from Next.js public env vars.
 */

import {
  Contract,
  Networks,
  nativeToScVal,
  SorobanRpc,
  TransactionBuilder,
} from "@stellar/stellar-sdk";

export const CONTRACT_ID = process.env.NEXT_PUBLIC_CONTRACT_ID ?? "";

export const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ??
  "https://soroban-testnet.stellar.org";

export const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  Networks.TESTNET;

export const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ??
  "https://horizon-testnet.stellar.org";

// ---------------------------------------------------------------------------
// Market reads (contract or indexer)
// ---------------------------------------------------------------------------

interface RpcMarket {
  id: string;
  question: string;
  yes_price: number;
  no_price: number;
  volume: string;
  status: string;
  ends_at: string;
}

interface GetMarketsResult {
  markets: RpcMarket[];
}

/**
 * Fetch markets from the contract via Soroban RPC.
 * Falls back to an empty array on error so the UI degrades gracefully.
 */
export async function fetchContractMarkets(): Promise<GetMarketsResult> {
  if (!CONTRACT_ID) {
    return { markets: [] };
  }
  try {
    const server = new SorobanRpc.Server(SOROBAN_RPC_URL);
    // `getContractData` is a low-level key/value read; the actual key depends
    // on your indexer. Adjust StorageKey to match your contract's storage layout.
    const result = await server.getContractData(
      CONTRACT_ID,
      nativeToScVal("MARKETS"),
      SorobanRpc.Durability.Persistent,
    );
    if (!result || !result.val) {
      return { markets: [] };
    }
    // Deserialisation depends on your indexer schema; return empty until wired.
    return { markets: [] };
  } catch {
    return { markets: [] };
  }
}

// ---------------------------------------------------------------------------
// Signed invocations
// ---------------------------------------------------------------------------

interface SendResult {
  hash: string;
  status: string;
}

/**
 * Invoke a contract function using Freighter for signing.
 *
 * Flow:
 *   1. Fetch the caller's account from Horizon (sequence number)
 *   2. Build an unsigned InvokeHostFunction transaction via stellar-sdk
 *   3. Simulate via Soroban RPC (fills resource fees)
 *   4. Sign with Freighter
 *   5. Submit via Soroban RPC
 */
export async function invokeContract(
  functionName: "deposit" | "withdraw",
  args: { amount: string; address: string },
): Promise<{ hash: string }> {
  if (!CONTRACT_ID) {
    throw new Error(
      "NEXT_PUBLIC_CONTRACT_ID is not set. Add it to your .env.local file.",
    );
  }

  const server = new SorobanRpc.Server(SOROBAN_RPC_URL);

  // 1. Load source account (needed for sequence number)
  const account = await server.getAccount(args.address);

  // 2. Build the transaction
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(
      contract.call(
        functionName,
        nativeToScVal(args.address, { type: "address" }),
        nativeToScVal(BigInt(args.amount), { type: "i128" }),
      ),
    )
    .setTimeout(30)
    .build();

  // 3. Simulate to get resource fees and footprint
  const sim = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  const preparedTx = SorobanRpc.assembleTransaction(tx, sim).build();

  // 4. Sign with Freighter
  const { signTransaction } = await import("@stellar/freighter-api");
  const { signedTxXdr, error } = await signTransaction(preparedTx.toXDR(), {
    networkPassphrase: NETWORK_PASSPHRASE,
    address: args.address,
  });
  if (error) throw new Error(error.message);

  // 5. Submit
  const sent = await server.sendTransaction(
    TransactionBuilder.fromXDR(signedTxXdr, NETWORK_PASSPHRASE),
  ) as SendResult;

  return { hash: sent.hash };
}
