// Unit tests for sync-frontend-env.ts env merge and empty-placeholder skip rules (#1054).
//
// Run with: `node --test --import tsx scripts/sync-frontend-env.test.ts`
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  fieldsFrom,
  mergeRegistryIntoEnvLines,
  setEnvVar,
  trimTrailingBlankLines,
  type DeploymentRegistry,
} from "./sync-frontend-env";

function makeRegistry(
  overrides: Partial<DeploymentRegistry["contracts"]> = {},
): DeploymentRegistry {
  return {
    network: "testnet",
    networkPassphrase: "Test SDF Network ; September 2015",
    rpcUrl: "https://soroban-testnet.stellar.org",
    contracts: {
      market: { contractId: "CMARKET_DEPLOYED", wasmHash: "hash1" },
      treasury: { contractId: "CTREASURY_DEPLOYED", wasmHash: "hash2" },
      resolution: { contractId: "CRESOLUTION_DEPLOYED", wasmHash: "hash3" },
      outcomeToken: { contractId: "COUTCOME_DEPLOYED", wasmHash: "hash4" },
      ...overrides,
    },
  };
}

test("setEnvVar appends a missing KEY=value line and returns true", () => {
  const lines = ["# comment", "OTHER_KEY=123"];
  const changed = setEnvVar(lines, "NEXT_PUBLIC_MARKET_CONTRACT_ID", "CMARKET");
  assert.equal(changed, true);
  assert.deepEqual(lines, [
    "# comment",
    "OTHER_KEY=123",
    "NEXT_PUBLIC_MARKET_CONTRACT_ID=CMARKET",
  ]);
});

test("setEnvVar updates a stale KEY=old line in place and preserves surrounding lines", () => {
  const lines = [
    "# comment before",
    "NEXT_PUBLIC_MARKET_CONTRACT_ID=COLD_ID",
    "# comment after",
  ];
  const changed = setEnvVar(
    lines,
    "NEXT_PUBLIC_MARKET_CONTRACT_ID",
    "CNEW_ID",
  );
  assert.equal(changed, true);
  assert.deepEqual(lines, [
    "# comment before",
    "NEXT_PUBLIC_MARKET_CONTRACT_ID=CNEW_ID",
    "# comment after",
  ]);
});

test("setEnvVar returns false (no-op) when KEY=value already matches", () => {
  const lines = ["NEXT_PUBLIC_MARKET_CONTRACT_ID=CSAME_ID"];
  const changed = setEnvVar(
    lines,
    "NEXT_PUBLIC_MARKET_CONTRACT_ID",
    "CSAME_ID",
  );
  assert.equal(changed, false);
  assert.deepEqual(lines, ["NEXT_PUBLIC_MARKET_CONTRACT_ID=CSAME_ID"]);
});

test("mergeRegistryIntoEnvLines skips empty registry placeholders and leaves existing local line byte-identical", () => {
  const localMarketLine =
    "NEXT_PUBLIC_MARKET_CONTRACT_ID=CLOCAL_WORKING_CONTRACT_ID";
  const initialLines = [
    "# Local overrides",
    localMarketLine,
    "NEXT_PUBLIC_TREASURY_CONTRACT_ID=CSTALE_TREASURY",
    "",
    "",
  ];

  const registry = makeRegistry({
    market: { contractId: "", wasmHash: "" },
  });

  const result = mergeRegistryIntoEnvLines(initialLines, registry);

  assert.ok(
    result.skippedEmpty.includes("NEXT_PUBLIC_MARKET_CONTRACT_ID"),
    "market contract ID must be recorded as skippedEmpty",
  );
  assert.ok(
    !result.changed.includes("NEXT_PUBLIC_MARKET_CONTRACT_ID"),
    "market contract ID must not be marked changed",
  );
  assert.equal(
    result.lines[1],
    localMarketLine,
    "existing local market contract ID line must remain byte-identical",
  );
  assert.ok(
    result.lines.includes(
      "NEXT_PUBLIC_TREASURY_CONTRACT_ID=CTREASURY_DEPLOYED",
    ),
  );
  assert.notEqual(
    result.lines[result.lines.length - 1],
    "",
    "trailing blank lines must be trimmed",
  );
});

test("fieldsFrom maps all six frontend env vars and trimTrailingBlankLines strips only trailing empty strings", () => {
  const fields = fieldsFrom(makeRegistry());
  assert.deepEqual(Object.keys(fields).sort(), [
    "NEXT_PUBLIC_MARKET_CONTRACT_ID",
    "NEXT_PUBLIC_NETWORK_PASSPHRASE",
    "NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID",
    "NEXT_PUBLIC_RESOLUTION_CONTRACT_ID",
    "NEXT_PUBLIC_SOROBAN_RPC_URL",
    "NEXT_PUBLIC_TREASURY_CONTRACT_ID",
  ]);
  assert.deepEqual(trimTrailingBlankLines(["a", "", "b", "", ""]), [
    "a",
    "",
    "b",
  ]);
});
