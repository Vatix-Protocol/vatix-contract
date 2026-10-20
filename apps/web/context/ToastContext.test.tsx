// @vitest-environment jsdom
//
// Unit tests for the ToastContext provider (Issue #1072).
//
// The provider schedules a `setTimeout(() => dismiss(id))` per toast. Before
// this fix the timer handle was discarded, so a manually dismissed toast (or
// an unmounted provider) could still fire `setToasts` afterwards. These tests
// pin the timer lifecycle.
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DISMISS_MS,
  ToastProvider,
  useToast,
  type ToastVariant,
} from "./ToastContext";

/** Renders the provider and exposes the context value to the test. */
function renderToasts() {
  const api: {
    showToast: (message: string, variant?: ToastVariant) => void;
    dismiss: (id: number) => void;
  } = {
    showToast: () => {},
    dismiss: () => {},
  };

  function Probe() {
    const toast = useToast();
    api.showToast = toast.showToast;
    api.dismiss = toast.dismiss;
    return null;
  }

  const utils = render(
    <ToastProvider>
      <Probe />
    </ToastProvider>,
  );
  return { api, ...utils };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ToastProvider", () => {
  it("shows a toast and auto-dismisses it after the dismiss delay", () => {
    const { api } = renderToasts();

    act(() => {
      api.showToast("boom");
    });
    expect(screen.getByText("boom")).toBeTruthy();

    act(() => {
      vi.advanceTimersByTime(DISMISS_MS.error + 1);
    });
    expect(screen.queryByText("boom")).toBeNull();
  });

  it("clears the pending timer when a toast is dismissed manually", () => {
    const { api } = renderToasts();

    act(() => {
      api.showToast("manual");
    });
    expect(screen.getByText("manual")).toBeTruthy();

    // Dismiss before the auto-dismiss timer fires.
    act(() => {
      api.dismiss(0);
    });
    expect(screen.queryByText("manual")).toBeNull();

    // The pending timer must have been cleared: advancing past the dismiss
    // delay must not cause any further state update (which would warn about
    // updating an unmounted component).
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    act(() => {
      vi.advanceTimersByTime(DISMISS_MS.error + 1);
    });
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it("clears all outstanding timers when the provider unmounts", () => {
    const { api, unmount } = renderToasts();

    act(() => {
      api.showToast("one");
      api.showToast("two");
    });
    expect(screen.getByText("one")).toBeTruthy();
    expect(screen.getByText("two")).toBeTruthy();

    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    unmount();

    act(() => {
      vi.advanceTimersByTime(DISMISS_MS.error + 1);
    });
    expect(errorSpy).not.toHaveBeenCalled();
    // No timers are left pending after unmount.
    expect(vi.getTimerCount()).toBe(0);
  });

  it("leaves the toast list unchanged when dismissing an already-removed id", () => {
    const { api } = renderToasts();

    act(() => {
      api.showToast("keep");
    });

    // Dismiss an id that was never shown.
    act(() => {
      api.dismiss(999);
    });
    expect(screen.getByText("keep")).toBeTruthy();

    // Dismissing the same id twice is idempotent.
    act(() => {
      api.dismiss(0);
    });
    expect(screen.queryByText("keep")).toBeNull();

    act(() => {
      api.dismiss(0);
    });
    expect(screen.queryByText("keep")).toBeNull();
  });
});
