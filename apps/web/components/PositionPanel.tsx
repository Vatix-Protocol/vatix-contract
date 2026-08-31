"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useWallet } from "@/context/WalletContext";
import { getPosition, type PositionData } from "@/lib/contract-client";
import { DepositForm } from "./DepositForm";
import { WithdrawForm } from "./WithdrawForm";
import { LoadingSkeleton } from "./LoadingSkeleton";

interface PositionPanelProps {
  /**
   * Market to read the connected wallet's live position for. When omitted
   * (e.g. a cross-market "your positions" summary), no live read is made.
   */
  marketId?: string;
}

const STROOPS_PER_UNIT = 10_000_000;

function formatShares(stroops: bigint): string {
  return (Number(stroops) / STROOPS_PER_UNIT).toLocaleString(undefined, {
    maximumFractionDigits: 2,
  });
}

/**
 * PositionPanel displays the connected wallet's live on-chain position for a
 * market, alongside deposit and withdraw controls.
 *
 * The position is read directly from the market contract (`get_position`)
 * whenever the connected address or market changes, so the panel always
 * reflects on-chain state rather than cached/local data.
 *
 * When the contract returns no position (null) — i.e. the user has never
 * deposited into this market — a graceful empty state is rendered with a
 * call-to-action that scrolls the user to the deposit form so they can
 * open a position without leaving the page.
 */
export function PositionPanel({ marketId }: PositionPanelProps) {
  const { address } = useWallet();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [position, setPosition] = useState<PositionData | null>(null);

  // Ref for the deposit form so the empty-state CTA can scroll it into view.
  const depositFormRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    if (!address || !marketId) {
      setPosition(null);
      setError(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const result = await getPosition(parseInt(marketId, 10), address);
      setPosition(result);
    } catch (err) {
      console.error("Failed to load position:", err);
      setError(err instanceof Error ? err.message : "Failed to load position.");
      setPosition(null);
    } finally {
      setIsLoading(false);
    }
  }, [address, marketId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** Scroll the deposit form into view and focus the amount input. */
  const handleDepositCTA = () => {
    if (depositFormRef.current) {
      depositFormRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
      const amountInput = depositFormRef.current.querySelector<HTMLInputElement>(
        "input[type='number']"
      );
      amountInput?.focus();
    }
  };

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2">
        {/* Attach the ref to the wrapper so we can scroll to the deposit form */}
        <div ref={depositFormRef}>
          <DepositForm marketId={marketId} />
        </div>
        {/*
         * Pass `refresh` as `onSuccess` so the position panel refetches
         * on-chain state immediately after a successful withdrawal, keeping
         * the UI in sync without a full page reload (#591).
         */}
        <WithdrawForm onSuccess={refresh} />
      </div>

      <div className="rounded-lg border border-slate-200 p-4 dark:border-slate-700 sm:p-6">
        <h2 className="text-base font-semibold sm:text-lg">Your position</h2>
        <div className="mt-4 min-h-[11rem]">
          {!address ? (
            <div className="text-center py-8">
              <p className="text-sm text-slate-600 dark:text-slate-400">
                Connect your wallet to view your position.
              </p>
            </div>
          ) : isLoading ? (
            <LoadingSkeleton />
          ) : error ? (
            <div className="text-center py-8">
              <p role="alert" className="text-sm text-red-600 dark:text-red-400">
                {error}
              </p>
              <button
                type="button"
                onClick={() => void refresh()}
                className="mt-3 text-sm text-indigo-600 hover:text-indigo-500 dark:text-indigo-400"
              >
                Retry
              </button>
            </div>
          ) : !position ? (
            /* Empty state: user has no open position in this market.
               Render a clear message and a CTA that scrolls to the
               deposit form so they can open one without hunting for it. */
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <svg
                aria-hidden="true"
                className="mb-3 h-10 w-10 text-slate-300 dark:text-slate-600"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={1.5}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"
                />
              </svg>
              <p className="text-sm font-medium text-slate-700 dark:text-slate-300">
                No open position
              </p>
              <p className="mt-1 text-xs text-slate-500 dark:text-slate-500">
                You don&apos;t have an active position in this market yet.
              </p>
              <button
                type="button"
                onClick={handleDepositCTA}
                className="mt-4 inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:bg-indigo-600 dark:hover:bg-indigo-500"
                aria-label="Deposit funds to open a position in this market"
              >
                Deposit to get started
              </button>
            </div>
          ) : (
            <dl className="grid grid-cols-2 gap-4 text-sm sm:grid-cols-4">
              <div>
                <dt className="text-slate-500 dark:text-slate-400">YES shares</dt>
                <dd className="mt-1 font-medium text-slate-900 dark:text-slate-100">
                  {formatShares(position.yesShares)}
                </dd>
              </div>
              <div>
                <dt className="text-slate-500 dark:text-slate-400">NO shares</dt>
                <dd className="mt-1 font-medium text-slate-900 dark:text-slate-100">
                  {formatShares(position.noShares)}
                </dd>
              </div>
              <div>
                <dt className="text-slate-500 dark:text-slate-400">Locked collateral</dt>
                <dd className="mt-1 font-medium text-slate-900 dark:text-slate-100">
                  {formatShares(position.lockedCollateral)}
                </dd>
              </div>
              <div>
                <dt className="text-slate-500 dark:text-slate-400">Status</dt>
                <dd className="mt-1 font-medium text-slate-900 dark:text-slate-100">
                  {position.isSettled ? "Settled" : "Open"}
                </dd>
              </div>
            </dl>
          )}
        </div>
      </div>
    </div>
  );
}
