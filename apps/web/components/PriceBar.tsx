interface PriceBarProps {
  yesPrice: number;
  noPrice?: number;
}

export function PriceBar({ yesPrice, noPrice }: PriceBarProps) {
  const yesPct = Math.round(yesPrice * 100);
  const noPct = noPrice ? Math.round(noPrice * 100) : 100 - yesPct;

  return (
    <div className="h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
      <div
        className="h-full bg-emerald-500"
        style={{ width: `${yesPct}%` }}
        role="progressbar"
        aria-valuenow={yesPct}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={`Yes ${yesPct}%`}
      />
    </div>
  );
}
