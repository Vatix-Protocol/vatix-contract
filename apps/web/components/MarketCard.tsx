import type { Market } from "@/lib/markets";

interface MarketCardProps {
  market: Market;
  loading?: boolean;
}

export function MarketCard({ market, loading = false }: MarketCardProps) {
  const yesPct = Math.round(market.yesPrice * 100);

  return (
    <article
      className={`rounded-xl border border-slate-200 p-4 dark:border-slate-700${
        loading ? " animate-pulse" : ""
      }`}
    >
      <p className="text-xs uppercase tracking-wide text-slate-500 break-words">
        {loading ? (
          <span className="inline-block h-3 w-40 rounded bg-slate-200 align-middle dark:bg-slate-800" />
        ) : (
          <>
            <span className="block sm:inline">
              {market.status}
            </span>
            <span className="hidden sm:inline mx-1">·</span>
            <span className="block sm:inline">
              ends {market.endsAt}
            </span>
          </>
        )}
      </p>
      <h3 className="mt-2 font-medium leading-snug break-words">
        {loading ? (
          <span className="block h-5 w-full rounded bg-slate-200 dark:bg-slate-800" />
        ) : (
          market.question
        )}
      </h3>
      <div className="mt-4 flex flex-col sm:flex-row sm:items-end sm:justify-between gap-3 text-sm">
        <div className="flex flex-wrap gap-2">
          {loading ? (
            <span className="inline-block h-4 w-28 rounded bg-slate-200 dark:bg-slate-800" />
          ) : (
            <>
              <span className="text-emerald-600 dark:text-emerald-400">
                Yes {yesPct}%
              </span>
              <span className="text-slate-400">·</span>
              <span className="text-rose-600 dark:text-rose-400">
                No {100 - yesPct}%
              </span>
            </>
          )}
        </div>
        {loading ? (
          <span className="inline-block h-4 w-16 rounded bg-slate-200 dark:bg-slate-800" />
        ) : (
          <span className="text-slate-500 whitespace-nowrap">Vol {market.volume}</span>
        )}
      </div>
      <div className="mt-3 h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
        {loading ? (
          <div className="h-full w-2/3 bg-slate-200 dark:bg-slate-700" />
        ) : (
          <div
            className="h-full bg-emerald-500"
            style={{ width: `${yesPct}%` }}
            role="progressbar"
            aria-valuenow={yesPct}
            aria-valuemin={0}
            aria-valuemax={100}
          />
        )}
      </div>
    </article>
  );
}
