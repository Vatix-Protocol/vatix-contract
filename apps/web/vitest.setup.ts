import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// React Testing Library's automatic cleanup only registers itself when the
// test globals are enabled. We run vitest with `globals: false`, so register
// the cleanup explicitly to keep each test's DOM isolated.
afterEach(() => {
  cleanup();
});
