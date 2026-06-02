"use client";

import { useState, useEffect } from "react";
import { DepositForm } from "./DepositForm";
import { WithdrawForm } from "./WithdrawForm";
import { LoadingSkeleton } from "./LoadingSkeleton";

/**
 * Props for the PositionPanel component.
 *
 * Currently no props are required — the panel manages its own loading and
 * position state internally. Props will be added here when the Soroban
 * contract integration lands and a wallet address or market list needs to
 * be passed in from a parent.
 *
 * @example
 * ```tsx
 * // Basic usage — wrap in an ErrorBoundary to catch unexpected throws
 * <ErrorBoundary>
 *   <PositionPanel />
 * </ErrorBoundary>
 * ```
 */
export interface PositionPanelProps {}

/**
 * PositionPanel displays the user's open positions alongside deposit and
 * withdraw forms. It simulates a loading delay on mount and then renders
 * either a skeleton, an empty-state message, or the list of positions.
 *
 * @param {PositionPanelProps} _props - No props currently required.
 *
 * @example
 * ```tsx
 * <PositionPanel />
 * ```
 */
export function PositionPanel(_props: PositionPanelProps = {}) {
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
        <div className="mt-4">
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
