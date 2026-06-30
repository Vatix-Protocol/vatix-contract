#!/usr/bin/env tsx
/**
 * Generate TypeScript contract bindings from compiled WASM files.
 *
 * This script:
 * 1. Validates prerequisites (stellar CLI present, version meets minimum)
 * 2. Builds all contracts using `stellar contract build`
 * 3. Validates that expected WASM artifacts were produced
 * 4. Generates TypeScript client bindings using `stellar contract bindings typescript`
 * 5. Places generated files in apps/web/lib/contracts/
 *
 * Prerequisites:
 * - stellar CLI ≥21.0.0 installed and in PATH
 * - Rust toolchain with wasm32-unknown-unknown target
 *
 * Usage:
 *   pnpm build:bindings
 *   tsx scripts/generate-bindings.ts
 *
 * Exit codes:
 *   0 – all bindings generated successfully
 *   1 – fatal error (missing prerequisite, build failure, no WASM output)
 */

import { execSync } from "child_process";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "fs";
import { join, basename } from "path";

const ROOT = join(__dirname, "..");
const CONTRACTS_DIR = join(ROOT, "contracts");
const WEB_CONTRACTS_OUTPUT = join(ROOT, "apps", "web", "lib", "contracts");
const WASM_TARGET = join(ROOT, "target", "wasm32v1-none", "release");

/** Minimum stellar CLI major version required. */
const MIN_STELLAR_MAJOR = 21;

interface ContractConfig {
  name: string;
  wasmStem: string; // vatix_<stem>_contract
  outputName: string;
}

const CONTRACTS: ContractConfig[] = [
  { name: "market", wasmStem: "market", outputName: "market" },
  { name: "treasury", wasmStem: "treasury", outputName: "treasury" },
  { name: "outcome-token", wasmStem: "outcome_token", outputName: "outcome-token" },
  { name: "resolution", wasmStem: "resolution", outputName: "resolution" },
];

// ---------------------------------------------------------------------------
// Logging helpers
// ---------------------------------------------------------------------------

function log(message: string): void {
  console.log(`[generate-bindings] ${message}`);
}

function warn(message: string): void {
  console.warn(`[generate-bindings] WARN: ${message}`);
}

function fatal(message: string): never {
  console.error(`[generate-bindings] ERROR: ${message}`);
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Execution helper
// ---------------------------------------------------------------------------

function exec(command: string, cwd: string = ROOT): string {
  try {
    return execSync(command, {
      cwd,
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
  } catch (err: unknown) {
    const e = err as { stderr?: string; message?: string };
    throw new Error(e.stderr?.trim() || e.message || String(err));
  }
}

// ---------------------------------------------------------------------------
// Prerequisite validation
// ---------------------------------------------------------------------------

function checkStellarCli(): void {
  let versionOutput: string;
  try {
    versionOutput = exec("stellar --version");
  } catch {
    fatal(
      "stellar CLI not found. Install it from https://developers.stellar.org/docs/tools/developer-tools\n" +
        "  curl -L https://github.com/stellar/stellar-cli/releases/download/v21.4.0/stellar-cli-21.4.0-x86_64-unknown-linux-gnu.tar.gz | tar xz\n" +
        "  sudo mv stellar /usr/local/bin/"
    );
  }

  // Parse version string like "stellar 21.4.0" or "stellar-cli 21.4.0"
  const match = versionOutput.match(/(\d+)\.(\d+)\.(\d+)/);
  if (!match) {
    warn(`Could not parse stellar CLI version from: "${versionOutput}". Proceeding anyway.`);
    return;
  }

  const major = parseInt(match[1], 10);
  if (major < MIN_STELLAR_MAJOR) {
    fatal(
      `stellar CLI v${match[0]} is below the minimum required v${MIN_STELLAR_MAJOR}.x.x.\n` +
        "Upgrade from https://developers.stellar.org/docs/tools/developer-tools"
    );
  }

  log(`Found stellar CLI: ${versionOutput}`);
}

function checkRustTarget(): void {
  try {
    const targets = exec("rustup target list --installed");
    if (
      !targets.includes("wasm32-unknown-unknown") &&
      !targets.includes("wasm32v1-none")
    ) {
      warn(
        "Neither wasm32-unknown-unknown nor wasm32v1-none Rust target is installed.\n" +
          "Run: rustup target add wasm32-unknown-unknown"
      );
    }
  } catch {
    // rustup not installed or unavailable – non-fatal, stellar contract build will surface the issue
    warn("Could not verify Rust wasm target (rustup not found). Build may fail.");
  }
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

function buildContracts(): void {
  log("Building contracts with `stellar contract build`…");
  try {
    exec("stellar contract build", ROOT);
  } catch (err) {
    fatal(`Contract build failed:\n${err}`);
  }

  if (!existsSync(WASM_TARGET)) {
    fatal(
      `WASM output directory not found after build: ${WASM_TARGET}\n` +
        "Ensure the Rust toolchain and wasm target are installed."
    );
  }

  const wasmFiles = readdirSync(WASM_TARGET).filter((f) => f.endsWith(".wasm"));
  if (wasmFiles.length === 0) {
    fatal(`No WASM files found in ${WASM_TARGET} after build.`);
  }

  log(`✓ Built ${wasmFiles.length} WASM file(s): ${wasmFiles.join(", ")}`);
}

// ---------------------------------------------------------------------------
// Validate individual WASM files
// ---------------------------------------------------------------------------

function resolveWasmPath(contract: ContractConfig): string | null {
  const wasmFile = join(WASM_TARGET, `vatix_${contract.wasmStem}_contract.wasm`);
  if (!existsSync(wasmFile)) {
    warn(
      `WASM not found for "${contract.name}" at ${basename(wasmFile)}. Skipping.`
    );
    return null;
  }
  return wasmFile;
}

// ---------------------------------------------------------------------------
// Bindings generation
// ---------------------------------------------------------------------------

function generateBindings(): { generated: number; skipped: number } {
  log("Generating TypeScript bindings…");

  if (!existsSync(WEB_CONTRACTS_OUTPUT)) {
    mkdirSync(WEB_CONTRACTS_OUTPUT, { recursive: true });
    log(`Created output directory: ${WEB_CONTRACTS_OUTPUT}`);
  }

  let generated = 0;
  let skipped = 0;

  for (const contract of CONTRACTS) {
    const wasmFile = resolveWasmPath(contract);
    if (!wasmFile) {
      skipped++;
      continue;
    }

    const contractIdPlaceholder = `${contract.outputName.toUpperCase().replace(/-/g, "_")}_CONTRACT`;
    const command =
      `stellar contract bindings typescript` +
      ` --wasm "${wasmFile}"` +
      ` --output-dir "${WEB_CONTRACTS_OUTPUT}"` +
      ` --contract-id ${contractIdPlaceholder}` +
      ` --overwrite`;

    log(`  Generating bindings for ${contract.name} (${basename(wasmFile)})…`);
    try {
      exec(command);
      log(`  ✓ ${contract.name}`);
      generated++;
    } catch (err) {
      warn(`Failed to generate bindings for "${contract.name}":\n  ${err}`);
      skipped++;
    }
  }

  return { generated, skipped };
}

// ---------------------------------------------------------------------------
// Index file
// ---------------------------------------------------------------------------

function writeIndexFile(): void {
  const indexContent = `/**
 * Auto-generated contract bindings index.
 *
 * Generated by scripts/generate-bindings.ts
 * Do not edit this file manually — run \`pnpm build:bindings\` to regenerate.
 */

// Re-export all contract clients (uncomment after running pnpm build:bindings)
// export * from './market';
// export * from './treasury';
// export * from './outcome-token';
// export * from './resolution';

// Contract IDs from environment variables
export const MARKET_CONTRACT_ID = process.env.NEXT_PUBLIC_MARKET_CONTRACT_ID ?? '';
export const TREASURY_CONTRACT_ID = process.env.NEXT_PUBLIC_TREASURY_CONTRACT_ID ?? '';
export const OUTCOME_TOKEN_CONTRACT_ID = process.env.NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID ?? '';
export const RESOLUTION_CONTRACT_ID = process.env.NEXT_PUBLIC_RESOLUTION_CONTRACT_ID ?? '';

// Network configuration
export const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  'Test SDF Network ; September 2015';

export const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ??
  'https://soroban-testnet.stellar.org';

export const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ??
  'https://horizon-testnet.stellar.org';
`;

  const indexPath = join(WEB_CONTRACTS_OUTPUT, "index.ts");
  writeFileSync(indexPath, indexContent, "utf-8");
  log(`✓ Wrote ${indexPath}`);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

function main(): void {
  log("Starting TypeScript bindings generation…\n");

  checkStellarCli();
  checkRustTarget();
  buildContracts();

  const { generated, skipped } = generateBindings();

  if (generated === 0 && CONTRACTS.length > 0) {
    fatal("No bindings were generated. Check the warnings above.");
  }

  writeIndexFile();

  log(`\n✓ Done — ${generated} binding(s) generated, ${skipped} skipped.`);
  log(`Output: ${WEB_CONTRACTS_OUTPUT}`);

  if (skipped > 0) {
    log("Some contracts were skipped. Run `stellar contract build` to rebuild missing WASMs.");
  }
}

if (require.main === module) {
  main();
}
