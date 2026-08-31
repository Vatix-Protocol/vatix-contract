"use client";

import { useState, useEffect } from "react";
import { useWallet } from "@/context/WalletContext";
import { invokeContract, MARKET_CONTRACT_ID, amountToScVal, addressToScVal, u32ToScVal, getPosition } from "@/lib/contract-client";
import { TxResult } from "@/components/TxResult";
import { useToast } from "@/context/ToastContext";
import { parseContractError } from "@/lib/errors";

interface WithdrawFormProps {
  /**
   * Optional callback invoked after a successful withdrawal transaction.
   * Use this to trigger a position refetch in the parent (e.g. PositionPanel).
   */
  onSuccess?: () => void;
}

export function WithdrawForm({ onSuccess }: WithdrawFormProps) {
  const { address } = useWallet();
  const { showToast } = useToast();
  const [amount, setAmount] = useState("");
  const [marketId, setMarketId] = useState("1");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [available, setAvailable] = useState<bigint | null>(null);

  useEffect(() => {
    if (address && marketId && !isNaN(parseInt(marketId))) {
      getPosition(parseInt(marketId), address)
        .then((pos) => {
          if (pos) {
            setAvailable(pos.totalDeposited - pos.lockedCollateral);
          } else {
            setAvailable(0n);
          }
        })
        .catch(console.error);
    } else {
      setAvailable(null);
    }
  }, [address, marketId]);

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
      // Convert amount to stroops (1 token = 10^7 stroops for USDC-like tokens)
      const amountInStroopsStr = Math.floor(parseFloat(amount) * 10_000_000).toString();
      
      if (available !== null && BigInt(amountInStroopsStr) > available) {
        const availableTokens = Number(available) / 10_000_000;
        setError(`Amount exceeds available collateral (${availableTokens})`);
        setIsLoading(false);
        return;
      }
      
      // Prepare contract arguments for withdraw_unused_collateral(market_id: u32, user: Address, amount: i128)
      const args = [
        u32ToScVal(parseInt(marketId)),
        addressToScVal(address),
        amountToScVal(amountInStroopsStr),
      ];

      // Invoke the withdraw_unused_collateral method
      const result = await invokeContract(
        MARKET_CONTRACT_ID,
        "withdraw_unused_collateral",
        args,
        address
      );

      setTxHash(result.hash);
      setAmount("");

      // Notify parent so it can refetch the position and reflect the updated
      // on-chain state without requiring a full page reload (#591).
      onSuccess?.();
    } catch (err) {
      console.error("Withdrawal error:", err);
      const reason = parseContractError(err);
      setError(reason);
      showToast(reason, "error");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="w-full rounded-lg border border-slate-200 p-4 dark:border-slate-700 sm:p-6"
    >
      <h3 className="text-base font-semibold sm:text-lg">Withdraw funds</h3>
      <div className="mt-4 space-y-4">
        <div>
          <label
            htmlFor="withdraw-market-id"
            className="block text-sm font-medium text-slate-700 dark:text-slate-300"
          >
            Market ID
          </label>
          <input
            id="withdraw-market-id"
            type="number"
            value={marketId}
            onChange={(e) => setMarketId(e.target.value)}
            placeholder="1"
            className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm placeholder-slate-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
          />
        </div>
        <div>
          <label
            htmlFor="withdraw-amount"
            className="block text-sm font-medium text-slate-700 dark:text-slate-300"
          >
            Amount
          </label>
          <input
            id="withdraw-amount"
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
        {txHash && <TxResult hash={txHash} label="Withdrawal" />}
        <button
          type="submit"
          disabled={isLoading || !amount || !address}
          aria-label={isLoading ? "Processing withdrawal" : "Withdraw funds"}
          className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed dark:bg-indigo-600 dark:hover:bg-indigo-500"
        >
          {isLoading ? "Processing..." : "Withdraw"}
        </button>
      </div>
    </form>
  );
}
