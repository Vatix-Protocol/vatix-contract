"use client";

import { useState } from "react";

export function DepositForm() {
  const [amount, setAmount] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    setTimeout(() => setIsLoading(false), 1000);
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="w-full rounded-lg border border-slate-200 p-4 dark:border-slate-700 sm:p-6"
    >
      <h3 className="text-base font-semibold sm:text-lg">Deposit funds</h3>
      <div className="mt-4 space-y-4">
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
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            placeholder="0.00"
            className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm placeholder-slate-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 dark:border-slate-600 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
          />
        </div>
        <button
          type="submit"
          disabled={isLoading || !amount}
          aria-label={isLoading ? "Processing deposit" : "Deposit funds"}
          className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed dark:bg-indigo-600 dark:hover:bg-indigo-500"
        >
          {isLoading ? "Processing..." : "Deposit"}
        </button>
      </div>
    </form>
  );
}
