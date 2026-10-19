import { useMemo } from "react";

interface PriceBarProps {
  yesPrice: number;
  noPrice?: number;
}

function clampPct(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.min(100, Math.max(0, Math.round(value * 100)));
}

export default function PriceBar({ yesPrice, noPrice }: PriceBarProps) {
  const yesPct = useMemo(() => clampPct(yesPrice), [yesPrice]);
  const noPct = useMemo(
    () => (noPrice === undefined ? undefined : clampPct(noPrice)),
    [noPrice]
  );

  const label =
    noPct === undefined ? `Yes ${yesPct}%` : `Yes ${yesPct}%, No ${noPct}%`;

  return (
    <div className="price-bar">
      <div
        className="price-bar__yes"
        style={{ width: `${yesPct}%` }}
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={yesPct}
      />
      {noPct !== undefined && (
        <div
          className="price-bar__no"
          style={{ width: `${noPct}%` }}
          role="progressbar"
          aria-label={`No ${noPct}%`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={noPct}
        />
      )}
    </div>
  );
}
