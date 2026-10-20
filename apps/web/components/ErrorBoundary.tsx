"use client";

import React from "react";

interface ErrorBoundaryProps {
  children: React.ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends React.Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    // Always report caught errors, in every environment, so production
    // failures are never silently swallowed. In development we also log the
    // extra component stack to speed up debugging.
    console.error("[ErrorBoundary]", {
      scope: "error-boundary",
      message: error.message,
      stack: error.stack,
    });
    if (process.env.NODE_ENV === "development") {
      console.error("Error info:", errorInfo);
    }
  }

  resetErrorBoundary = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): React.ReactNode {
    if (this.state.hasError) {
      return (
        <div
          className="rounded-lg border border-rose-200 bg-rose-50 p-4 dark:border-rose-900 dark:bg-rose-950"
          role="alert"
        >
          <p className="font-medium text-rose-900 dark:text-rose-200">
            Something went wrong
          </p>
          <p className="mt-1 text-sm text-rose-800 dark:text-rose-300">
            We encountered an error while rendering this component. You can
            try again or refresh the page.
          </p>
          <div className="mt-3 flex gap-2">
            <button
              onClick={this.resetErrorBoundary}
              className="rounded px-3 py-1 text-sm font-medium text-rose-700 hover:bg-rose-100 focus:outline-none focus:ring-2 focus:ring-rose-500 focus:ring-offset-2 dark:text-rose-300 dark:hover:bg-rose-900 dark:focus:ring-offset-rose-950"
              aria-label="Try again"
            >
              Try Again
            </button>
            <button
              onClick={() => window.location.reload()}
              className="rounded px-3 py-1 text-sm font-medium text-rose-700 hover:bg-rose-100 focus:outline-none focus:ring-2 focus:ring-rose-500 focus:ring-offset-2 dark:text-rose-300 dark:hover:bg-rose-900 dark:focus:ring-offset-rose-950"
              aria-label="Refresh page to recover from error"
            >
              Refresh Page
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
