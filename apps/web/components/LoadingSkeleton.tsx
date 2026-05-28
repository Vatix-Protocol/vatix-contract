"use client";

export function LoadingSkeleton() {
  return (
    <div className="space-y-3">
      {[...Array(3)].map((_, i) => (
        <div
          key={i}
          className="h-12 animate-pulse rounded-lg bg-slate-200 dark:bg-slate-700"
        />
      ))}
    </div>
  );
}
