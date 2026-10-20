import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import WalletConnectButton, {
  FreighterNetworkError,
  STELLAR_NETWORK_PASSPHRASES,
  resolveExpectedPassphrase,
  validateNetworkPassphrase,
} from "./WalletConnectButton";

function installFreighter(overrides: Partial<{
  isConnected: () => Promise<{ isConnected: boolean }>;
  requestAccess: () => Promise<{ address: string }>;
  getNetwork: () => Promise<{ network: string; networkPassphrase: string }>;
  getPublicKey: () => Promise<{ publicKey: string }>;
}> = {}) {
  const freighter = {
    isConnected: vi.fn().mockResolvedValue({ isConnected: true }),
    requestAccess: vi.fn().mockResolvedValue({ address: "GADDRESS" }),
    getNetwork: vi.fn().mockResolvedValue({
      network: "TESTNET",
      networkPassphrase: STELLAR_NETWORK_PASSPHRASES.TESTNET,
    }),
    getPublicKey: vi.fn().mockResolvedValue({ publicKey: "GPUBKEY" }),
    ...overrides,
  };
  (window as unknown as { freighter?: unknown }).freighter = freighter;
  return freighter;
}

describe("resolveExpectedPassphrase", () => {
  it("fails closed when configuration is missing", () => {
    expect(resolveExpectedPassphrase(undefined)).toEqual({
      ok: false,
      code: FreighterNetworkError.MISSING_CONFIG,
    });
  });

  it("fails closed on an unrecognized network value", () => {
    expect(resolveExpectedPassphrase("MOONNET")).toEqual({
      ok: false,
      code: FreighterNetworkError.UNSUPPORTED_NETWORK,
    });
  });
});

describe("validateNetworkPassphrase", () => {
  it("fails closed when the actual passphrase is missing", () => {
    expect(
      validateNetworkPassphrase(STELLAR_NETWORK_PASSPHRASES.TESTNET, null),
    ).toEqual({ ok: false, code: FreighterNetworkError.NETWORK_MISMATCH });
  });
});

describe("WalletConnectButton", () => {
  afterEach(() => {
    delete (window as unknown as { freighter?: unknown }).freighter;
    vi.restoreAllMocks();
  });

  it("connects when the wallet resolves to the expected network", async () => {
    installFreighter();
    const onConnected = vi.fn();

    render(
      <WalletConnectButton expectedNetwork="TESTNET" onConnected={onConnected} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect Freighter" }));

    await waitFor(() => expect(onConnected).toHaveBeenCalledWith("GPUBKEY"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("fails closed and blocks connection when the wallet network does not match", async () => {
    installFreighter({
      getNetwork: vi.fn().mockResolvedValue({
        network: "PUBLIC",
        networkPassphrase: STELLAR_NETWORK_PASSPHRASES.PUBLIC,
      }),
    });
    const onConnected = vi.fn();
    const onError = vi.fn();

    render(
      <WalletConnectButton
        expectedNetwork="TESTNET"
        onConnected={onConnected}
        onError={onError}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect Freighter" }));

    await waitFor(() =>
      expect(onError).toHaveBeenCalledWith(FreighterNetworkError.NETWORK_MISMATCH),
    );
    expect(onConnected).not.toHaveBeenCalled();
    expect(
      screen.getByRole("alert").getAttribute("data-error-code"),
    ).toBe(FreighterNetworkError.NETWORK_MISMATCH);
  });

  it("fails closed when network resolution throws instead of returning unknown", async () => {
    installFreighter({
      getNetwork: vi.fn().mockRejectedValue(new Error("rpc unavailable")),
    });
    const onConnected = vi.fn();
    const onError = vi.fn();

    render(
      <WalletConnectButton
        expectedNetwork="TESTNET"
        onConnected={onConnected}
        onError={onError}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect Freighter" }));

    await waitFor(() =>
      expect(onError).toHaveBeenCalledWith(FreighterNetworkError.CONNECT_FAILED),
    );
    expect(onConnected).not.toHaveBeenCalled();
  });

  it("fails closed when the configured expected network is unsupported", async () => {
    installFreighter();
    const onConnected = vi.fn();
    const onError = vi.fn();

    render(
      <WalletConnectButton
        expectedNetwork="MOONNET"
        onConnected={onConnected}
        onError={onError}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect Freighter" }));

    await waitFor(() =>
      expect(onError).toHaveBeenCalledWith(FreighterNetworkError.UNSUPPORTED_NETWORK),
    );
    expect(onConnected).not.toHaveBeenCalled();
  });
});
