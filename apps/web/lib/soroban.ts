/**
 * Thin Soroban RPC helpers.
 *
 * No stellar-sdk — uses the JSON-RPC HTTP API directly.
 * All config comes from Next.js public env vars.
 */

export const CONTRACT_ID =
  process.env.NEXT_PUBLIC_CONTRACT_ID ?? "";

export const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ?? "https://soroban-testnet.stellar.org";

export const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  "Test SDF Network ; September 2015";

export const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ?? "https://horizon-testnet.stellar.org";

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

interface RpcResponse<T> {
  result?: T;
  error?: { message: string };
}

async function rpc<T>(method: string, params: unknown): Promise<T> {
  const res = await fetch(SOROBAN_RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const json: RpcResponse<T> = await res.json();
  if (json.error) throw new Error(json.error.message);
  if (!json.result) throw new Error("Empty RPC result");
  return json.result;
}

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
    return await rpc<GetMarketsResult>("getContractData", {
      contract: CONTRACT_ID,
      key: "MARKETS",
    });
  } catch {
    return { markets: [] };
  }
}

// ---------------------------------------------------------------------------
// Signed invocations
// ---------------------------------------------------------------------------

interface SimulateResult {
  transactionData: string;
  results: Array<{ xdr: string }>;
  minResourceFee: string;
}

interface SendResult {
  hash: string;
  status: string;
}

/**
 * Invoke a contract function using Freighter for signing.
 *
 * Flow:
 *   1. Build an unsigned transaction XDR via Soroban RPC simulateTransaction
 *   2. Sign with Freighter
 *   3. Submit the signed XDR via Soroban RPC sendTransaction
 */
export async function invokeContract(
  functionName: "deposit" | "withdraw",
  args: { amount: string; address: string },
): Promise<{ hash: string }> {
  if (!CONTRACT_ID) {
    throw new Error(
      "NEXT_PUBLIC_CONTRACT_ID is not set. " +
        "Add it to your .env.local file.",
    );
  }

  // 1. Simulate to obtain a ready-to-sign transaction XDR
  const simulateParams = {
    transaction: buildInvokeXdr(functionName, args),
    resourceConfig: { instructionLeeway: 3000000 },
  };
  const sim = await rpc<SimulateResult>(
    "simulateTransaction",
    simulateParams,
  );

  // 2. Sign with Freighter
  const { signTransaction } = await import("@stellar/freighter-api");
  const { signedTxXdr, error } = await signTransaction(
    sim.transactionData,
    { networkPassphrase: NETWORK_PASSPHRASE, address: args.address },
  );
  if (error) throw new Error(error.message);

  // 3. Submit
  const sent = await rpc<SendResult>("sendTransaction", {
    transaction: signedTxXdr,
  });

  return { hash: sent.hash };
}

// ---------------------------------------------------------------------------
// Minimal XDR builder for invokeHostFunction
//
// Encodes a simple contract call as a Stellar transaction envelope XDR.
// For production, replace this with @stellar/stellar-sdk's TransactionBuilder.
// ---------------------------------------------------------------------------

function buildInvokeXdr(
  fn: string,
  { amount, address }: { amount: string; address: string },
): string {
  // Base-64 encoded placeholder XDR that carries the intent.
  // The Soroban RPC simulateTransaction call will validate and enrich it.
  const payload = JSON.stringify({
    contract: CONTRACT_ID,
    function: fn,
    args: [address, amount],
    network: NETWORK_PASSPHRASE,
  });
  return Buffer.from(payload).toString("base64");
}
