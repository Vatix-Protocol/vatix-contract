"use client";

import React from "react";

interface DarkModeErrorBoundaryProps {
  children: React.ReactNode;
}

interface DarkModeErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class DarkModeErrorBoundary extends React.Component<
  DarkModeErrorBoundaryProps,
  DarkModeErrorBoundaryState
> {
  constructor(props: DarkModeErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): DarkModeErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    if (process.env.NODE_ENV === "development") {
      console.error("Dark mode error:", error);
      console.error("Error info:", errorInfo);
    }
  }

  /**
   * Clears the error state so the subtree is re-mounted/re-rendered on the
   * next render. Without this, once an error is caught the boundary stays
   * stuck showing the fallback for the rest of the session (#1103), since
   * nothing ever flips `hasError` back to false.
   */
  reset = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): React.ReactNode {
    if (this.state.hasError) {
      return (
        <div className="rounded-lg border border-slate-200 bg-slate-50 p-4 dark:border-slate-700 dark:bg-slate-900">
          <p className="font-medium text-slate-900 dark:text-slate-100">
            Dark mode styling error
          </p>
          <p className="mt-1 text-sm text-slate-700 dark:text-slate-300">
            An error occurred while applying dark mode styles. Using fallback styling.
          </p>
          <button
            type="button"
            onClick={this.reset}
            className="mt-3 text-sm font-medium text-indigo-600 hover:text-indigo-500 dark:text-indigo-400"
          >
            Retry
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
