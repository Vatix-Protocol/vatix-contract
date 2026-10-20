import type { ReactElement } from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Bomb({ shouldThrow }: { shouldThrow: boolean }): ReactElement {
  if (shouldThrow) {
    throw new Error("boom");
  }
  return <div>safe content</div>;
}

describe("ErrorBoundary", () => {
  let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleErrorSpy.mockRestore();
  });

  it("renders children when there is no error", () => {
    render(
      <ErrorBoundary>
        <Bomb shouldThrow={false} />
      </ErrorBoundary>
    );

    expect(screen.getByText("safe content")).toBeInTheDocument();
  });

  it("reports the caught error even in production", () => {
    const originalEnv = process.env.NODE_ENV;
    // @ts-expect-error - NODE_ENV is read-only in types but writable at runtime
    process.env.NODE_ENV = "production";

    render(
      <ErrorBoundary>
        <Bomb shouldThrow={true} />
      </ErrorBoundary>
    );

    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(consoleErrorSpy).toHaveBeenCalledWith(
      "[ErrorBoundary]",
      expect.objectContaining({
        scope: "error-boundary",
        message: "boom",
      })
    );

    // @ts-expect-error - restoring for other tests
    process.env.NODE_ENV = originalEnv;
  });

  it("recovers a child that has since stopped throwing once Try Again resets the boundary", () => {
    let shouldThrow = true;

    function Toggleable(): ReactElement {
      if (shouldThrow) {
        throw new Error("boom");
      }
      return <div>recovered content</div>;
    }

    const { rerender } = render(
      <ErrorBoundary>
        <Toggleable />
      </ErrorBoundary>
    );

    expect(screen.getByText("Something went wrong")).toBeInTheDocument();

    // The underlying condition is fixed, but the boundary still shows the
    // fallback until it is explicitly reset.
    shouldThrow = false;
    rerender(
      <ErrorBoundary>
        <Toggleable />
      </ErrorBoundary>
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Try again" }));

    expect(screen.getByText("recovered content")).toBeInTheDocument();
  });
});
