interface EmptyStateProps {
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
}

export function EmptyState({
  title,
  description,
  actionLabel,
  onAction,
}: EmptyStateProps) {
  return (
    <div className="w-full max-w-xl mx-auto flex flex-col items-center justify-center rounded-lg border border-slate-200 bg-slate-50 p-6 sm:p-8 text-center dark:border-slate-700 dark:bg-slate-900">
      <h3 className="text-base sm:text-lg font-medium text-slate-900 dark:text-slate-100">
        {title}
      </h3>
      <p className="mt-2 text-sm sm:text-base text-slate-600 dark:text-slate-400">
        {description}
      </p>
      {actionLabel && onAction && (
        <button
          onClick={onAction}
          aria-label={actionLabel}
          className="mt-4 rounded-lg bg-blue-600 px-3 sm:px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 dark:focus:ring-offset-slate-900 w-full sm:w-auto max-w-xs"
        >
          {actionLabel}
        </button>
      )}
      {actionLabel && !onAction && (
        <a
          href="#"
          aria-label={actionLabel}
          className="mt-4 rounded-lg bg-blue-600 px-3 sm:px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 dark:focus:ring-offset-slate-900 w-full sm:w-auto max-w-xs"
        >
          {actionLabel}
        </a>
      )}
    </div>
  );
}
