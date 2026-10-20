// @vitest-environment jsdom
//
// Unit tests for the TxResult component (Issue #1071).
//
// Covers the two failure modes of the original implementation:
//  1. A base URL with a trailing slash produced `.../tx//<hash>`.
//  2. An empty or malformed hash still rendered a clickable link to the
//     explorer root, which looked like a successful confirmation.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import {
  DEFAULT_EXPLORER_BASE,
  TxResult,
  buildExplorerTxUrl,
  isPlausibleTxHash,
  normalizeExplorerBase,
} from "./TxResult";

/** A canonical 32-byte Stellar transaction hash rendered as 64 hex chars. */
const VALID_HASH =
  "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

describe("normalizeExplorerBase", () => {
  it("strips a single trailing slash", () => {
    expect(normalizeExplorerBase("https://stellar.expert/explorer/testnet/tx/")).toBe(
      "https://stellar.expert/explorer/testnet/tx",
    );
  });

  it("strips multiple trailing slashes", () => {
    expect(normalizeExplorerBase("https://stellar.expert/explorer/testnet/tx///")).toBe(
      "https://stellar.expert/explorer/testnet/tx",
    );
  });

  it("leaves a base URL without a trailing slash untouched", () => {
    expect(normalizeExplorerBase("https://stellar.expert/explorer/testnet/tx")).toBe(
      "https://stellar.expert/explorer/testnet/tx",
    );
  });
});

describe("isPlausibleTxHash", () => {
  it("accepts a 64-character hex hash", () => {
    expect(isPlausibleTxHash(VALID_HASH)).toBe(true);
  });

  it("rejects an empty hash", () => {
    expect(isPlausibleTxHash("")).toBe(false);
  });

  it("rejects a non-hex hash", () => {
    expect(isPlausibleTxHash("z".repeat(64))).toBe(false);
  });

  it("rejects a truncated hash", () => {
    expect(isPlausibleTxHash(VALID_HASH.slice(0, 63))).toBe(false);
  });
});

describe("buildExplorerTxUrl", () => {
  it("joins a base URL with a trailing slash using a single slash", () => {
    const url = buildExplorerTxUrl(
      "https://stellar.expert/explorer/testnet/tx/",
      VALID_HASH,
    );
    expect(url).toBe(`https://stellar.expert/explorer/testnet/tx/${VALID_HASH}`);
    expect(url).not.toContain("tx//");
  });

  it("joins a base URL without a trailing slash using a single slash", () => {
    expect(buildExplorerTxUrl(DEFAULT_EXPLORER_BASE, VALID_HASH)).toBe(
      `${DEFAULT_EXPLORER_BASE}/${VALID_HASH}`,
    );
  });

  it("returns null for an empty hash", () => {
    expect(buildExplorerTxUrl(DEFAULT_EXPLORER_BASE, "")).toBeNull();
  });

  it("returns null for a malformed hash", () => {
    expect(buildExplorerTxUrl(DEFAULT_EXPLORER_BASE, "not-a-hash")).toBeNull();
  });
});

describe("<TxResult />", () => {
  it("renders an anchor whose href ends with the hash", () => {
    render(<TxResult hash={VALID_HASH} />);
    const link = screen.getByRole("link");
    expect(link.getAttribute("href")).toBe(`${DEFAULT_EXPLORER_BASE}/${VALID_HASH}`);
    expect(link.getAttribute("href")?.endsWith(VALID_HASH)).toBe(true);
  });

  it("renders no anchor for an empty hash", () => {
    render(<TxResult hash="" />);
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByTestId("tx-hash-plain")).toBeTruthy();
  });

  it("renders no anchor for a malformed hash", () => {
    render(<TxResult hash="not-a-real-hash" />);
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("renders the label and a shortened hash", () => {
    render(<TxResult hash={VALID_HASH} label="Deposit" />);
    expect(screen.getByText(/Deposit confirmed:/)).toBeTruthy();
  });
});
