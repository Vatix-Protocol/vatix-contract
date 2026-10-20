import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "."),
    },
  },
  test: {
    // The web app is a React app, so the default test environment is jsdom.
    // Pure-logic test files can opt back into `node` with a
    // `// @vitest-environment node` pragma.
    environment: "jsdom",
    include: [
      "**/*.{test,spec}.{ts,tsx}",
      "**/__tests__/**/*.{ts,tsx}",
    ],
    // `lib/errors.test.ts` is written for Node's built-in test runner
    // (`node --test --import tsx`) and imports the *generated* contract
    // bindings under `lib/contracts/`, which only exist after
    // `pnpm build:bindings` has run against compiled WASM. It is therefore not
    // part of the Vitest suite.
    exclude: [
      "node_modules/**",
      ".next/**",
      "lib/errors.test.ts",
    ],
    setupFiles: ["./vitest.setup.ts"],
    globals: false,
    restoreMocks: true,
  },
});
