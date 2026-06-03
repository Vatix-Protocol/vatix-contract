/**
 * StatusBadge component props
 * @typedef {Object} StatusBadgeProps
 * @property {string} status - The status to display (e.g., "active", "resolved", "expired")
 * @property {string} [variant] - Visual variant: "default" | "success" | "warning" | "error" (default: "default")
 * @property {string} [className] - Additional CSS classes to apply
 */
interface StatusBadgeProps {
  status: string;
  variant?: "default" | "success" | "warning" | "error";
  className?: string;
}

/**
 * StatusBadge - Displays a status indicator with visual styling
 *
 * @example
 * // Basic usage
 * <StatusBadge status="Active" />
 *
 * @example
 * // With variant
 * <StatusBadge status="Resolved" variant="success" />
 *
 * @example
 * // With custom styling
 * <StatusBadge status="Expired" variant="error" className="text-lg" />
 */
export function StatusBadge({
  status,
  variant = "default",
  className = "",
}: StatusBadgeProps) {
  const variantStyles = {
    default: "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-100",
    success: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-100",
    warning: "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-100",
    error: "bg-rose-100 text-rose-700 dark:bg-rose-900 dark:text-rose-100",
  };

  return (
    <span
      className={`inline-block rounded-full px-3 py-1 text-xs font-medium ${variantStyles[variant]} ${className}`}
    >
      {status}
    </span>
  );
}
