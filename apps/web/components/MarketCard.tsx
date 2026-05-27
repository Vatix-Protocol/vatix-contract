import type { Market } from "@/lib/markets";

interface MarketCardProps {
  market: Market;
}

export function MarketCard({ market }: MarketCardProps) {
  const yesPct = Math.round(market.yesPrice * 100);

  return (
    <article className="rounded-xl border border-slate-200 p-4 dark:border-slate-700">
      <p className="text-xs uppercase tracking-wide text-slate-500">
        {market.status} · ends {market.endsAt}
      </p>
      <h3 className="mt-2 font-medium leading-snug">{market.question}</h3>
      <div className="mt-4 flex items-end justify-between text-sm">
        <div>
          <span className="text-emerald-600 dark:text-emerald-400">
            Yes {yesPct}%
          </span>
          <span className="mx-2 text-slate-400">·</span>
          <span className="text-rose-600 dark:text-rose-400">
            No {100 - yesPct}%
          </span>
        </div>
        <span className="text-slate-500">Vol {market.volume}</span>
      </div>
      <div className="mt-3 h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
        <div
          className="h-full bg-emerald-500"
          style={{ width: `${yesPct}%` }}
          role="progressbar"
          aria-valuenow={yesPct}
          aria-valuemin={0}
          aria-valuemax={100}
        />
      </div>
    </article>
  );
}
