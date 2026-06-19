"use client";

import { useState } from "react";
import { useWallet } from "../context/WalletContext";

export function WithdrawForm() {
  const [amount, setAmount] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { withdraw } = useWallet();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    setError(null);
    try {
      await withdraw(amount);
      setAmount("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "An error occurred");
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
            htmlFor="withdraw-amount"
            className="block text-sm font-medium text-slate-700 dark:text-slate-300"
          >
            Amount
          </label>
          <input
            id="withdraw-amount"
            type="number"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            placeholder="0.00"
            className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm placeholder-slate-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
          />
        </div>
        {error && (
          <div className="rounded-lg bg-red-50 p-3 text-sm text-red-600 dark:bg-red-900/20 dark:text-red-400">
            {error}
          </div>
        )}
        <button
          type="submit"
          disabled={isLoading || !amount}
          aria-label={isLoading ? "Processing withdrawal" : "Withdraw funds"}
          className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed dark:bg-indigo-600 dark:hover:bg-indigo-500"
        >
          {isLoading ? "Processing..." : "Withdraw"}
        </button>
      </div>
    </form>
  );
}
