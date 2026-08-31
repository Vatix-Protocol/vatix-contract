#!/usr/bin/env tsx
/**
 * Sync `apps/web/.env.local`'s contract IDs from `deployments/testnet.json`
 * (Issue #700).
 *
 * `deployments/testnet.json` is the single source of truth for deployed
 * contract IDs (see `deployments/README.md`); `.env.local` previously had to
 * be filled in by hand by copy-pasting from it, which drifts silently after
 * a redeploy — a stale or empty `NEXT_PUBLIC_*_CONTRACT_ID` doesn't fail
 * loudly, it just makes `invokeContract`/`queryContract` throw "Contract ID
 * not configured" at call time, or (worse) point at a decommissioned
 * contract. This script closes that gap by writing the registry's values
 * into `.env.local` directly, so `.env.local` can never drift from
 * `deployments/testnet.json` without an explicit, visible sync.
 *
 * Registry values that are still empty placeholders (not yet deployed) are
 * never written over an existing local value — this script only ever moves
 * data from "real deployed ID" to `.env.local`, never blanks out a working
 * config.
 *
 * Usage:
 *   pnpm env:sync
 *   tsx scripts/sync-frontend-env.ts
 */

import { existsSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";

const ROOT = join(__dirname, "..");
const REGISTRY_PATH = join(ROOT, "deployments", "testnet.json");
const ENV_EXAMPLE_PATH = join(ROOT, "apps", "web", ".env.local.example");
const ENV_LOCAL_PATH = join(ROOT, "apps", "web", ".env.local");

interface DeploymentRegistry {
  network: string;
  networkPassphrase: string;
  rpcUrl: string;
  contracts: {
    market: { contractId: string; wasmHash: string };
    treasury: { contractId: string; wasmHash: string };
    resolution: { contractId: string; wasmHash: string };
    outcomeToken: { contractId: string; wasmHash: string };
  };
}

/** Env vars this script owns, mapped to where their value comes from in the registry. */
function fieldsFrom(registry: DeploymentRegistry): Record<string, string> {
  return {
    NEXT_PUBLIC_MARKET_CONTRACT_ID: registry.contracts.market.contractId,
    NEXT_PUBLIC_TREASURY_CONTRACT_ID: registry.contracts.treasury.contractId,
    NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID: registry.contracts.outcomeToken.contractId,
    NEXT_PUBLIC_RESOLUTION_CONTRACT_ID: registry.contracts.resolution.contractId,
    NEXT_PUBLIC_SOROBAN_RPC_URL: registry.rpcUrl,
    NEXT_PUBLIC_NETWORK_PASSPHRASE: registry.networkPassphrase,
  };
}

function setEnvVar(lines: string[], key: string, value: string): boolean {
  const pattern = new RegExp(`^${key}=`);
  const idx = lines.findIndex((l) => pattern.test(l));
  const newLine = `${key}=${value}`;
  if (idx === -1) {
    lines.push(newLine);
    return true;
  }
  if (lines[idx] === newLine) {
    return false;
  }
  lines[idx] = newLine;
  return true;
}

function main(): void {
  if (!existsSync(REGISTRY_PATH)) {
    console.error(`ERROR: registry not found at ${REGISTRY_PATH}`);
    process.exit(1);
  }
  const registry: DeploymentRegistry = JSON.parse(readFileSync(REGISTRY_PATH, "utf8"));

  const baseContent = existsSync(ENV_LOCAL_PATH)
    ? readFileSync(ENV_LOCAL_PATH, "utf8")
    : existsSync(ENV_EXAMPLE_PATH)
      ? readFileSync(ENV_EXAMPLE_PATH, "utf8")
      : "";
  const lines = baseContent.length > 0 ? baseContent.split("\n") : [];

  const changed: string[] = [];
  const skippedEmpty: string[] = [];

  for (const [key, value] of Object.entries(fieldsFrom(registry))) {
    if (value === "" || value === undefined) {
      skippedEmpty.push(key);
      continue;
    }
    if (setEnvVar(lines, key, value)) {
      changed.push(key);
    }
  }

  // Trim any trailing blank lines introduced by the join, keep a single
  // trailing newline.
  while (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }
  writeFileSync(ENV_LOCAL_PATH, lines.join("\n") + "\n");

  console.log(`Synced ${ENV_LOCAL_PATH} from ${REGISTRY_PATH}`);
  if (changed.length > 0) {
    console.log(`  Updated: ${changed.join(", ")}`);
  } else {
    console.log("  No changes — already in sync.");
  }
  if (skippedEmpty.length > 0) {
    console.log(
      `  Not yet deployed in the registry (left untouched): ${skippedEmpty.join(", ")}`,
    );
  }
}

main();
