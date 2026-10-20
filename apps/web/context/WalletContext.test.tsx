// @vitest-environment jsdom
//
// Unit tests for the WalletContext network verification (Issue #1073).
//
// The network check used to run only at connect time, so a mid-session switch
// of the Freighter network went unnoticed. `refreshNetwork` re-runs the same
// verification and fails closed on a mismatch.
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  NETWORK_PASSPHRASES,
  WalletProvider,
  resolveExpectedNetwork,
  resolveNetworkFromPassphrase,
  useWallet,
  type WalletErrorCode,
} from "./WalletContext";

const freighterMock = vi.hoisted(() => ({
  isConnected: vi.fn(),
  getAddress: vi.fn(),
  getNetwork: vi.fn(),
}));

vi.mock("@stellar/freighter-api", () => ({
  isConnected: freighterMock.isConnected,
  getAddress: freighterMock.getAddress,
  getNetwork: freighterMock.getNetwork,
}));

const TESTNET_PASSPHRASE = NETWORK_PASSPHRASES.TESTNET;
const MAINNET_PASSPHRASE = NETWORK_PASSPHRASES.PUBLIC;
const ADDRESS = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

interface WalletApi {
  connect: () => Promise<void>;
  refreshNetwork: () => Promise<void>;
  disconnect: () => void;
  connectErrorCode: WalletErrorCode | null;
  network: string | null;
  address: string | null;
}

function renderWallet(): { api: WalletApi; unmount: () => void } {
  const api: WalletApi = {
    connect: async () => {},
    refreshNetwork: async () => {},
    disconnect: () => {},
    connectErrorCode: null,
    network: null,
    address: null,
  };

  function Probe() {
    const wallet = useWallet();
    api.connect = wallet.connect;
    api.refreshNetwork = wallet.refreshNetwork;
    api.disconnect = wallet.disconnect;
    api.connectErrorCode = wallet.connectErrorCode;
    api.network = wallet.network;
    api.address = wallet.address;
    return (
      <span data-testid="error-code">{wallet.connectErrorCode ?? "none"}</span>
    );
  }

  const { unmount } = render(
    <WalletProvider>
      <Probe />
    </WalletProvider>,
  );
  return { api, unmount };
}

beforeEach(() => {
  // The default configured network is TESTNET.
  process.env.NEXT_PUBLIC_STELLAR_NETWORK = "TESTNET";
  freighterMock.isConnected.mockReset();
  freighterMock.getAddress.mockReset();
  freighterMock.getNetwork.mockReset();
  freighterMock.isConnected.mockResolvedValue({ isConnected: true });
  freighterMock.getAddress.mockResolvedValue({ address: ADDRESS });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("resolveNetworkFromPassphrase", () => {
  it("maps the canonical mainnet passphrase to PUBLIC", () => {
    expect(resolveNetworkFromPassphrase(MAINNET_PASSPHRASE)).toBe("PUBLIC");
  });

  it("maps the canonical testnet passphrase to TESTNET", () => {
    expect(resolveNetworkFromPassphrase(TESTNET_PASSPHRASE)).toBe("TESTNET");
  });

  it("returns null for an unrecognized passphrase", () => {
    expect(resolveNetworkFromPassphrase("Some Other Network")).toBeNull();
  });

  it("returns null for a missing passphrase", () => {
    expect(resolveNetworkFromPassphrase(null)).toBeNull();
  });
});

describe("resolveExpectedNetwork", () => {
  it("defaults to TESTNET when the env var is missing", () => {
    delete process.env.NEXT_PUBLIC_STELLAR_NETWORK;
    expect(resolveExpectedNetwork()).toBe("TESTNET");
  });

  it("maps MAINNET to PUBLIC", () => {
    process.env.NEXT_PUBLIC_STELLAR_NETWORK = "mainnet";
    expect(resolveExpectedNetwork()).toBe("PUBLIC");
  });
});

describe("WalletProvider network verification", () => {
  it("connects successfully when the wallet network matches", async () => {
    freighterMock.getNetwork.mockResolvedValue({
      network: TESTNET_PASSPHRASE,
      networkPassphrase: TESTNET_PASSPHRASE,
    });

    const { api } = renderWallet();
    await act(async () => {
      await api.connect();
    });

    expect(api.connectErrorCode).toBeNull();
    expect(api.network).toBe("TESTNET");
    expect(api.address).toBe(ADDRESS);
  });

  it("sets WALLET_NETWORK_MISMATCH when the wallet is on mainnet but testnet is expected", async () => {
    freighterMock.getNetwork.mockResolvedValue({
      network: MAINNET_PASSPHRASE,
      networkPassphrase: MAINNET_PASSPHRASE,
    });

    const { api } = renderWallet();
    await act(async () => {
      await api.connect();
    });

    expect(api.connectErrorCode).toBe("WALLET_NETWORK_MISMATCH");
    expect(api.network).toBeNull();
    expect(api.address).toBeNull();
  });

  it("sets WALLET_NETWORK_UNKNOWN for an unrecognized passphrase", async () => {
    freighterMock.getNetwork.mockResolvedValue({
      network: "Some Other Network",
      networkPassphrase: "Some Other Network",
    });

    const { api } = renderWallet();
    await act(async () => {
      await api.connect();
    });

    expect(api.connectErrorCode).toBe("WALLET_NETWORK_UNKNOWN");
    expect(api.network).toBeNull();
  });

  it("detects a mid-session network switch via refreshNetwork", async () => {
    freighterMock.getNetwork.mockResolvedValue({
      network: TESTNET_PASSPHRASE,
      networkPassphrase: TESTNET_PASSPHRASE,
    });

    const { api } = renderWallet();
    await act(async () => {
      await api.connect();
    });
    expect(api.network).toBe("TESTNET");

    // The user switches Freighter to mainnet while the app is open.
    freighterMock.getNetwork.mockResolvedValue({
      network: MAINNET_PASSPHRASE,
      networkPassphrase: MAINNET_PASSPHRASE,
    });

    await act(async () => {
      await api.refreshNetwork();
    });

    expect(api.connectErrorCode).toBe("WALLET_NETWORK_MISMATCH");
    expect(api.network).toBeNull();
    expect(api.address).toBeNull();
    expect(screen.getByTestId("error-code").textContent).toBe(
      "WALLET_NETWORK_MISMATCH",
    );
  });

  it("removes the focus listener on unmount", async () => {
    const removeSpy = vi.spyOn(window, "removeEventListener");
    const addSpy = vi.spyOn(window, "addEventListener");

    const { unmount } = renderWallet();

    const focusAdds = addSpy.mock.calls.filter(([type]) => type === "focus");
    expect(focusAdds.length).toBeGreaterThan(0);

    unmount();

    const focusRemoves = removeSpy.mock.calls.filter(([type]) => type === "focus");
    expect(focusRemoves.length).toBe(focusAdds.length);

    // The exact handler registered must be the one removed.
    const addedHandler = focusAdds[0][1] as EventListener;
    const removedHandler = focusRemoves[0][1] as EventListener;
    expect(removedHandler).toBe(addedHandler);
  });

  it("re-checks the wallet network when the window regains focus", async () => {
    freighterMock.getNetwork.mockResolvedValue({
      network: TESTNET_PASSPHRASE,
      networkPassphrase: TESTNET_PASSPHRASE,
    });

    const { api } = renderWallet();
    await act(async () => {
      await api.connect();
    });
    expect(api.network).toBe("TESTNET");

    freighterMock.getNetwork.mockResolvedValue({
      network: MAINNET_PASSPHRASE,
      networkPassphrase: MAINNET_PASSPHRASE,
    });

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      // Let the async verification settle.
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(api.connectErrorCode).toBe("WALLET_NETWORK_MISMATCH");
  });
});

