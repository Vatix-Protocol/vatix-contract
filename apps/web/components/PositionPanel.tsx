"use client";

export function PositionPanel() {
  return (
    <div className="rounded-lg border border-slate-200 p-6 dark:border-slate-700">
      <h2 className="font-semibold">Positions</h2>
      <p className="mt-2 text-sm text-slate-600 dark:text-slate-400">
        You have no open positions.
      </p>
    </div>
  );
}
