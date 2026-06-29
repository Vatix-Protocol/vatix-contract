"use client";

export function LoadingSkeleton() {
  return (
    <ul className="mt-8 grid gap-4 sm:grid-cols-2">
      {[...Array(4)].map((_, i) => (
        <li key={i} className="animate-pulse rounded-xl border border-slate-200 p-4 dark:border-slate-700">
          <div className="h-3 w-24 rounded bg-slate-200 dark:bg-slate-800" />
          <div className="mt-2 h-5 w-full rounded bg-slate-200 dark:bg-slate-800" />
          <div className="mt-1 h-5 w-3/4 rounded bg-slate-200 dark:bg-slate-800" />
          <div className="mt-4 flex justify-between">
            <div className="h-4 w-28 rounded bg-slate-200 dark:bg-slate-800" />
            <div className="h-4 w-16 rounded bg-slate-200 dark:bg-slate-800" />
          </div>
          <div className="mt-3 h-2 rounded-full bg-slate-100 dark:bg-slate-800" />
        </li>
      ))}
    </ul>
  );
}
