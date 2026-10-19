"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

export type ToastVariant = "error" | "success";

interface Toast {
  id: number;
  message: string;
  variant: ToastVariant;
}

export interface ToastState {
  /** Show a toast. Auto-dismisses after ~6s (errors) or ~4s (success). */
  showToast: (message: string, variant?: ToastVariant) => void;
  /**
   * Dismiss a toast by id. Idempotent: dismissing an id that is no longer
   * present leaves the toast list unchanged and cancels any pending timer.
   */
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastState | null>(null);

export const DISMISS_MS: Record<ToastVariant, number> = {
  error: 6000,
  success: 4000,
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(0);
  /**
   * Pending auto-dismiss timers, keyed by toast id. Tracking the handles lets
   * us cancel a toast's timer when it is dismissed manually and cancel every
   * outstanding timer when the provider unmounts, so a timer can never fire
   * `setToasts` on an unmounted component.
   */
  const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: number) => {
    // Cancel the pending auto-dismiss timer, if any. Clearing it here makes a
    // manual dismiss followed by the timer (or the reverse) idempotent.
    const timer = timers.current.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
    setToasts((current) => current.filter((t) => t.id !== id));
  }, []);

  const showToast = useCallback(
    (message: string, variant: ToastVariant = "error") => {
      const id = nextId.current++;
      setToasts((current) => [...current, { id, message, variant }]);
      const timer = setTimeout(() => {
        // Drop the handle before dismissing so the entry is not left behind.
        timers.current.delete(id);
        dismiss(id);
      }, DISMISS_MS[variant]);
      timers.current.set(id, timer);
    },
    [dismiss]
  );

  // Clear every outstanding timer on unmount (route change, error-boundary
  // reset) so nothing fires into an unmounted provider.
  useEffect(() => {
    const pending = timers.current;
    return () => {
      for (const timer of pending.values()) {
        clearTimeout(timer);
      }
      pending.clear();
    };
  }, []);

  const value = useMemo(() => ({ showToast, dismiss }), [showToast, dismiss]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        aria-live="assertive"
        className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex flex-col items-center gap-2 px-4 sm:items-end sm:px-6"
      >
        {toasts.map((toast) => (
          <div
            key={toast.id}
            role="alert"
            className={`pointer-events-auto w-full max-w-sm rounded-lg border px-4 py-3 text-sm shadow-lg ${
              toast.variant === "error"
                ? "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
                : "border-green-200 bg-green-50 text-green-800 dark:border-green-900 dark:bg-green-950 dark:text-green-200"
            }`}
          >
            <div className="flex items-start justify-between gap-3">
              <span className="break-words">{toast.message}</span>
              <button
                type="button"
                onClick={() => dismiss(toast.id)}
                aria-label="Dismiss notification"
                className="shrink-0 opacity-70 hover:opacity-100"
              >
                ×
              </button>
            </div>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastState {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return ctx;
}
