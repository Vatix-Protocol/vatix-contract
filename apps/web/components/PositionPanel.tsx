"use client";

import { useCallback, useEffect, useState } from "react";
import { useWallet } from "@/context/WalletContext";
import { getPosition, type PositionData } from "@/lib/contract-client";
import { parseContractError } from "@/lib/errors";
import { useToast } from "@/context/ToastContext";
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
 * Simulation failures (e.g. contract errors returned during the read-only
 * query) are parsed through {@link parseContractError} and surfaced both as
 * an inline alert and via the global toast so users see a recoverable,
 * human-readable message rather than a raw SDK error string.
 */
export function PositionPanel({ marketId }: PositionPanelProps) {
  const { address } = useWallet();
  const { showToast } = useToast();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [position, setPosition] = useState<PositionData | null>(null);

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
      // Parse the error into a short, user-facing message (handles Soroban
      // host traps, simulation failures, and generic network errors) then
      // surface it both inline and via the global toast banner.
      const reason = parseContractError(err);
      setError(reason);
      showToast(reason, "error");
      setPosition(null);
    } finally {
      setIsLoading(false);
    }
  }, [address, marketId, showToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2">
        <DepositForm marketId={marketId} />
        <WithdrawForm />
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
            <div className="text-center py-8">
              <p className="text-sm text-slate-600 dark:text-slate-400">
                You have no open position in this market yet.
              </p>
              <p className="mt-2 text-xs text-slate-500 dark:text-slate-500">
                Deposit funds above to get started.
              </p>
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
