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
    if (process.env.NODE_ENV === "development") {
      console.error("Error caught by boundary:", error);
      console.error("Error info:", errorInfo);
    }
  }

  render(): React.ReactNode {
    if (this.state.hasError) {
      return (
        <div className="rounded-lg border border-rose-200 bg-rose-50 p-4 dark:border-rose-900 dark:bg-rose-950">
          <p className="font-medium text-rose-900 dark:text-rose-200">
            Something went wrong
          </p>
          <p className="mt-1 text-sm text-rose-800 dark:text-rose-300">
            We encountered an error while rendering this component. Please try
            refreshing the page.
          </p>
        </div>
      );
    }

    return this.props.children;
  }
}
