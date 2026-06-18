/**
 * Generates TypeScript bindings for Soroban contracts.
 * Builds the contract and runs stellar contract bindings typescript.
 */

import { execSync } from "node:child_process";
import { mkdirSync, existsSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = resolve(__dirname, "..");
const CONTRACT_DIR = join(PROJECT_ROOT, "contracts", "market");
const BINDINGS_OUTPUT_DIR = join(PROJECT_ROOT, "apps", "web", "lib", "bindings");

function main() {
  console.log("Building Soroban contract...");
  execSync("make build", { cwd: CONTRACT_DIR, stdio: "inherit" });

  const wasmPath = join(
    PROJECT_ROOT,
    "contracts",
    "market",
    "target",
    "wasm32-unknown-unknown",
    "release",
    "vatix_market_contract.wasm",
  );

  if (!existsSync(wasmPath)) {
    throw new Error(`WASM file not found at: ${wasmPath}`);
  }

  console.log(`Generating TypeScript bindings from ${wasmPath}...`);

  if (existsSync(BINDINGS_OUTPUT_DIR)) {
    rmSync(BINDINGS_OUTPUT_DIR, { recursive: true, force: true });
  }
  mkdirSync(BINDINGS_OUTPUT_DIR, { recursive: true });

  execSync(
    `stellar contract bindings typescript --wasm ${wasmPath} --output-dir ${BINDINGS_OUTPUT_DIR} --overwrite`,
    { cwd: PROJECT_ROOT, stdio: "inherit" },
  );

  console.log("TypeScript bindings generated successfully!");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
