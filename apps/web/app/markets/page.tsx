import { MarketCard } from "@/components/MarketCard";
import { MOCK_MARKETS } from "@/lib/markets";
import { containerClass } from "@/lib/responsive";

export default function MarketsPage() {
  return (
    <div className={`${containerClass()} py-10`}>
      <h1 className="text-2xl font-semibold">All markets</h1>
      <p className="mt-1 text-sm text-slate-600 dark:text-slate-400">
        Mock data for UI development — replace with Soroban contract reads.
      </p>
      {MOCK_MARKETS.length === 0 ? (
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
      ) : (
        <ul className="mt-8 grid gap-4 sm:grid-cols-2">
          {MOCK_MARKETS.map((market) => (
            <li key={market.id}>
              <MarketCard market={market} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
