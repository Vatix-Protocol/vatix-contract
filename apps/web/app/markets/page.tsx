import { Suspense } from "react";
import { MarketCard } from "@/components/MarketCard";
import { LoadingSkeleton } from "@/components/LoadingSkeleton";
import { getMarkets, MOCK_MARKETS } from "@/lib/markets";
import { containerClass } from "@/lib/responsive";

async function MarketList() {
  const fetched = await getMarkets();
  const markets = fetched.length > 0 ? fetched : MOCK_MARKETS;

  if (markets.length === 0) {
    return (
      <div className="mt-8 rounded-xl border border-slate-200 p-6 text-sm dark:border-slate-700">
        <p className="font-medium">No markets yet</p>
        <p className="mt-1 text-slate-600 dark:text-slate-400">
          Check back soon, or browse the home page for updates.
        </p>
        <a
          href="/"
          className="mt-4 inline-flex rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500"
        >
          Back to home
        </a>
      </div>
    );
  }

  return (
    <ul className="mt-8 grid gap-4 sm:grid-cols-2">
      {markets.map((market) => (
        <li key={market.id}>
          <MarketCard market={market} />
        </li>
      ))}
    </ul>
  );
}

export default function MarketsPage() {
  return (
    <div className={`${containerClass()} py-10`}>
      <h1 className="text-2xl font-semibold">All markets</h1>
      <p className="mt-1 text-sm text-slate-600 dark:text-slate-400">
        Mock data for UI development — replace with Soroban contract reads.
      </p>
      <Suspense fallback={<LoadingSkeleton />}>
        <MarketList />
      </Suspense>
    </div>
  );
}
