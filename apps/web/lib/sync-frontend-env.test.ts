// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  fieldsFrom,
  mergeRegistryIntoEnvLines,
  setEnvVar,
  trimTrailingBlankLines,
  type DeploymentRegistry,
} from "../../../scripts/sync-frontend-env";

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

describe("sync-frontend-env (#1054)", () => {
  it("appends a missing KEY=value line and returns true", () => {
    const lines = ["# comment", "OTHER_KEY=123"];
    expect(setEnvVar(lines, "NEXT_PUBLIC_MARKET_CONTRACT_ID", "CMARKET")).toBe(
      true,
    );
    expect(lines).toEqual([
      "# comment",
      "OTHER_KEY=123",
      "NEXT_PUBLIC_MARKET_CONTRACT_ID=CMARKET",
    ]);
  });

  it("updates a stale KEY=old line in place while preserving surrounding comments", () => {
    const lines = [
      "# header",
      "NEXT_PUBLIC_MARKET_CONTRACT_ID=COLD",
      "# footer",
    ];
    expect(setEnvVar(lines, "NEXT_PUBLIC_MARKET_CONTRACT_ID", "CNEW")).toBe(
      true,
    );
    expect(lines).toEqual([
      "# header",
      "NEXT_PUBLIC_MARKET_CONTRACT_ID=CNEW",
      "# footer",
    ]);
  });

  it("returns false (no-op) when KEY=value already matches", () => {
    const lines = ["NEXT_PUBLIC_MARKET_CONTRACT_ID=CSAME"];
    expect(setEnvVar(lines, "NEXT_PUBLIC_MARKET_CONTRACT_ID", "CSAME")).toBe(
      false,
    );
    expect(lines).toEqual(["NEXT_PUBLIC_MARKET_CONTRACT_ID=CSAME"]);
  });

  it("skips empty registry placeholders and leaves existing local line byte-identical", () => {
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

    expect(result.skippedEmpty).toContain("NEXT_PUBLIC_MARKET_CONTRACT_ID");
    expect(result.changed).not.toContain("NEXT_PUBLIC_MARKET_CONTRACT_ID");
    expect(result.lines[1]).toBe(localMarketLine);
    expect(result.lines).toContain(
      "NEXT_PUBLIC_TREASURY_CONTRACT_ID=CTREASURY_DEPLOYED",
    );
    expect(result.lines[result.lines.length - 1]).not.toBe("");
  });

  it("maps all owned fields and trims trailing blank lines", () => {
    expect(Object.keys(fieldsFrom(makeRegistry())).sort()).toEqual([
      "NEXT_PUBLIC_MARKET_CONTRACT_ID",
      "NEXT_PUBLIC_NETWORK_PASSPHRASE",
      "NEXT_PUBLIC_OUTCOME_TOKEN_CONTRACT_ID",
      "NEXT_PUBLIC_RESOLUTION_CONTRACT_ID",
      "NEXT_PUBLIC_SOROBAN_RPC_URL",
      "NEXT_PUBLIC_TREASURY_CONTRACT_ID",
    ]);
    expect(trimTrailingBlankLines(["a", "", "b", "", ""])).toEqual([
      "a",
      "",
      "b",
    ]);
  });
});
