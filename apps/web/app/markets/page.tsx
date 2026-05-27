import { MarketCard } from "@/components/MarketCard";
import { MOCK_MARKETS } from "@/lib/markets";

export default function MarketsPage() {
  return (
    <div className="mx-auto max-w-4xl px-4 py-10">
      <h1 className="text-2xl font-semibold">All markets</h1>
      <p className="mt-1 text-sm text-slate-600 dark:text-slate-400">
        Mock data for UI development — replace with Soroban contract reads.
      </p>
      <ul className="mt-8 grid gap-4 sm:grid-cols-2">
        {MOCK_MARKETS.map((market) => (
          <li key={market.id}>
            <MarketCard market={market} />
          </li>
        ))}
      </ul>
    </div>
  );
}
