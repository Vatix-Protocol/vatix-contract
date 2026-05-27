import Link from "next/link";
import { MarketCard } from "@/components/MarketCard";
import { MOCK_MARKETS } from "@/lib/markets";

export default function HomePage() {
  return (
    <div className="mx-auto max-w-4xl px-4 py-10">
      <section className="mb-10">
        <h1 className="text-3xl font-semibold tracking-tight">
          Vatix prediction markets
        </h1>
        <p className="mt-2 max-w-xl text-sm text-slate-600 dark:text-slate-400">
          Trade binary Yes/No outcomes on Stellar. Connect a wallet to deposit
          and open positions (Soroban integration coming soon).
        </p>
        <Link
          href="/markets"
          className="mt-4 inline-flex rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-2"
        >
          Browse markets
        </Link>
      </section>

      <h2 className="mb-4 text-lg font-medium">Featured markets</h2>
      <ul className="grid gap-4 sm:grid-cols-2">
        {MOCK_MARKETS.slice(0, 2).map((market) => (
          <li key={market.id}>
            <MarketCard market={market} />
          </li>
        ))}
      </ul>
    </div>
  );
}
