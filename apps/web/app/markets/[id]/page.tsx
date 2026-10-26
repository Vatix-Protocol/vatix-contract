import { findMarketById } from "@/lib/market-helpers";
import { PositionPanel } from "@/components/PositionPanel";

export default async function MarketDetailPage({
  params,
}: {
  params: { id: string };
}) {
  const result = await findMarketById(params.id);

  if (result.kind === "invalid-id") {
    return (
      <div className="mx-auto max-w-4xl px-4 py-10">
        <h1 className="text-2xl font-semibold">Invalid market id</h1>
        <p className="mt-1 text-sm text-slate-600 dark:text-slate-400">
          &ldquo;{result.id}&rdquo; is not a valid market id.
        </p>
        <a
          href="/markets"
          className="mt-4 inline-flex rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500"
        >
          Back to markets
        </a>
      </div>
    );
  }

  if (result.kind === "not-found") {
    return (
      <div className="mx-auto max-w-4xl px-4 py-10">
        <h1 className="text-2xl font-semibold">Market not found</h1>
        <p className="mt-1 text-sm text-slate-600 dark:text-slate-400">
          The market you&apos;re looking for doesn&apos;t exist.
        </p>
        <a
          href="/markets"
          className="mt-4 inline-flex rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500"
        >
          Back to markets
        </a>
      </div>
    );
  }

  const market = result.market;

  return (
    <div className="mx-auto max-w-4xl px-4 py-10">
      <a
        href="/markets"
        className="text-sm text-indigo-600 hover:text-indigo-500 dark:text-indigo-400"
      >
        ← Back to markets
      </a>
      <h1 className="mt-4 text-3xl font-semibold">{market.question}</h1>
      <div className="mt-4 text-sm text-slate-600 dark:text-slate-400">
        <p>Status: {market.status}</p>
        <p>Ends: {market.endsAt}</p>
        <p>Volume: {market.volume}</p>
      </div>
      <div className="mt-8">
        <PositionPanel marketId={market.id} />
      </div>
    </div>
  );
}
