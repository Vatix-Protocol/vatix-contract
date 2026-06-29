"use client";

import { useState } from "react";
import { useWallet } from "@/context/WalletContext";
import { invokeContract, MARKET_CONTRACT_ID } from "@/lib/soroban";
import { amountToScVal, addressToScVal, u32ToScVal } from "@/lib/contract-client";

interface DepositFormProps {
  /** Pre-select a market. When provided the market ID field is hidden. */
  marketId?: string;
}

export function DepositForm({ marketId: marketIdProp }: DepositFormProps) {
  const { address } = useWallet();
  const [amount, setAmount] = useState("");
  const [marketId, setMarketId] = useState(marketIdProp ?? "1");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!address) {
      setError("Connect your Freighter wallet first.");
      return;
    }

    if (!MARKET_CONTRACT_ID) {
      setError("Market contract ID not configured. Set NEXT_PUBLIC_MARKET_CONTRACT_ID in .env.local");
      return;
    }

    setIsLoading(true);
    setError(null);
    setTxHash(null);

    try {
      const amountInStroops = Math.floor(parseFloat(amount) * 10_000_000).toString();
      const args = [
        u32ToScVal(parseInt(marketId)),
        addressToScVal(address),
        amountToScVal(amountInStroops),
      ];

      const result = await invokeContract(
        MARKET_CONTRACT_ID,
        "deposit_collateral",
        args,
        address
      );

      setTxHash(result.hash);
      setAmount("");
    } catch (err) {
      console.error("Deposit error:", err);
      setError(err instanceof Error ? err.message : "Deposit failed.");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="w-full rounded-lg border border-slate-200 p-4 dark:border-slate-700 sm:p-6"
    >
      <h3 className="text-base font-semibold sm:text-lg">Deposit funds</h3>
      <div className="mt-4 space-y-4">
        {!marketIdProp && (
          <div>
            <label
              htmlFor="market-id"
              className="block text-sm font-medium text-slate-700 dark:text-slate-300"
            >
              Market ID
            </label>
            <input
              id="market-id"
              type="number"
              value={marketId}
              onChange={(e) => setMarketId(e.target.value)}
              placeholder="1"
              className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm placeholder-slate-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
            />
          </div>
        )}
        <div>
          <label
            htmlFor="amount"
            className="block text-sm font-medium text-slate-700 dark:text-slate-300"
          >
            Amount
          </label>
          <input
            id="amount"
            type="number"
            step="0.01"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            placeholder="0.00"
            className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm placeholder-slate-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
          />
        </div>
        {error && (
          <p role="alert" className="text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}
        {txHash && (
          <p className="text-sm text-green-600 dark:text-green-400">
            Deposited! Tx: {txHash.slice(0, 8)}...{txHash.slice(-8)}
          </p>
        )}
        <button
          type="submit"
          disabled={isLoading || !amount || !address}
          aria-label={isLoading ? "Processing deposit" : "Deposit funds"}
          className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed dark:bg-indigo-600 dark:hover:bg-indigo-500"
        >
          {isLoading ? "Processing..." : "Deposit"}
        </button>
      </div>
    </form>
  );
}
