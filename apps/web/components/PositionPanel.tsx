"use client";

import { useState, useEffect } from "react";
import { DepositForm } from "./DepositForm";
import { WithdrawForm } from "./WithdrawForm";
import { LoadingSkeleton } from "./LoadingSkeleton";

export function PositionPanel() {
  const [isLoading, setIsLoading] = useState(true);
  const [positions, setPositions] = useState<any[]>([]);

  useEffect(() => {
    const timer = setTimeout(() => {
      setIsLoading(false);
      setPositions([]);
    }, 1500);

    return () => clearTimeout(timer);
  }, []);

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2">
        <DepositForm />
        <WithdrawForm />
      </div>

      <div className="rounded-lg border border-slate-200 p-4 dark:border-slate-700 sm:p-6">
        <h2 className="text-base font-semibold sm:text-lg">Your positions</h2>
        <div className="mt-4 min-h-[11rem]">
          {isLoading ? (
            <LoadingSkeleton />
          ) : positions.length === 0 ? (
            <div className="text-center py-8">
              <p className="text-sm text-slate-600 dark:text-slate-400">
                You have no open positions yet.
              </p>
              <p className="mt-2 text-xs text-slate-500 dark:text-slate-500">
                Deposit funds and browse markets to get started.
              </p>
            </div>
          ) : (
            <ul className="space-y-2">
              {positions.map((pos, i) => (
                <li key={i} className="text-sm text-slate-700 dark:text-slate-300">
                  {pos.name}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
