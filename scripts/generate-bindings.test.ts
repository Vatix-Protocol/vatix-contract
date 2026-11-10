// Unit tests for checkRustTarget and WASM target directory resolution (#1053).
//
// Run with: `node --test --import tsx scripts/generate-bindings.test.ts`
import { test } from "node:test";
import assert from "node:assert/strict";
import { join } from "node:path";
import {
  checkRustTarget,
  resolveWasmTargetDir,
  SUPPORTED_WASM_TARGETS,
} from "./generate-bindings";

test("checkRustTarget calls fatal (exits non-zero) when neither wasm32v1-none nor wasm32-unknown-unknown is installed", () => {
  let fatalMessage = "";
  assert.throws(
    () =>
      checkRustTarget({
        execFn: () => "x86_64-unknown-linux-gnu\naarch64-apple-darwin",
        fatalFn: (msg) => {
          fatalMessage = msg;
          throw new Error("fatal-exit");
        },
      }),
    /fatal-exit/,
  );
  assert.match(fatalMessage, /wasm32v1-none/);
  assert.match(fatalMessage, /wasm32-unknown-unknown/);
  assert.match(fatalMessage, /rustup target add/);
});

test("checkRustTarget succeeds when wasm32v1-none or wasm32-unknown-unknown is installed", () => {
  let fatalCalled = false;
  for (const target of SUPPORTED_WASM_TARGETS) {
    checkRustTarget({
      execFn: () => `x86_64-unknown-linux-gnu\n${target}`,
      fatalFn: () => {
        fatalCalled = true;
        throw new Error("should not call fatal");
      },
    });
  }
  assert.equal(fatalCalled, false);
});

test("checkRustTarget warns (does not fail) when rustup is unavailable", () => {
  const warnings: string[] = [];
  checkRustTarget({
    execFn: () => {
      throw new Error("rustup: command not found");
    },
    warnFn: (msg) => warnings.push(msg),
    fatalFn: () => {
      throw new Error("must not call fatal when rustup is absent");
    },
  });
  assert.equal(warnings.length, 1);
  assert.match(warnings[0]!, /rustup not found/);
});

test("resolveWasmTargetDir reconciles wasm32v1-none and wasm32-unknown-unknown output directories", () => {
  const root = "/repo";
  const v1Dir = join(root, "target", "wasm32v1-none", "release");
  const unknownDir = join(root, "target", "wasm32-unknown-unknown", "release");

  // Prefers wasm32v1-none when present
  assert.equal(
    resolveWasmTargetDir(root, (p) => p === v1Dir || p === unknownDir),
    v1Dir,
  );

  // Falls back to wasm32-unknown-unknown when only wasm32-unknown-unknown exists
  assert.equal(
    resolveWasmTargetDir(root, (p) => p === unknownDir),
    unknownDir,
  );

  // Defaults to wasm32v1-none path when neither directory exists yet
  assert.equal(
    resolveWasmTargetDir(root, () => false),
    v1Dir,
  );
});
