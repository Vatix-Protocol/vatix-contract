"use client";

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/**
 * Stable error codes surfaced by the wallet integration. Consumers can branch
 * on these instead of parsing human-readable messages.
 */
export type WalletErrorCode =
  | "WALLET_NOT_INSTALLED"
  | "WALLET_NOT_CONNECTED"
  | "WALLET_CONNECTION_REJECTED"
  | "WALLET_NO_ADDRESS"
  | "WALLET_UNAVAILABLE"
  | "WALLET_NETWORK_MISMATCH"
  | "WALLET_NETWORK_UNKNOWN";

export interface WalletError {
  code: WalletErrorCode;
  message: string;
}

/**
 * Canonical Stellar network passphrases. Freighter reports the network it is
 * currently pointed at; we compare it against the network this app is
 * configured for and fail closed on any mismatch.
 */
export const NETWORK_PASSPHRASES = {
  PUBLIC: "Public Global Stellar Network ; September 2015",
  TESTNET: "Test SDF Network ; September 2015",
  FUTURENET: "Test SDF Future Network ; October 2022",
} as const;

export type StellarNetwork = keyof typeof NETWORK_PASSPHRASES;

/**
 * Resolve the expected network from the build-time env. Defaults to TESTNET so
 * that a missing/blank configuration never silently targets mainnet.
 */
export function resolveExpectedNetwork(): StellarNetwork {
  const raw = (process.env.NEXT_PUBLIC_STELLAR_NETWORK ?? "").trim().toUpperCase();
  if (raw === "PUBLIC" || raw === "MAINNET") return "PUBLIC";
  if (raw === "FUTURENET") return "FUTURENET";
  return "TESTNET";
}

/**
 * Map a Freighter-reported network identifier (passphrase or short name) to a
 * known network. Returns null when the value cannot be recognized, which the
 * caller treats as a fail-closed mismatch.
 */
export function resolveNetworkFromPassphrase(
  passphrase: string | null | undefined,
): StellarNetwork | null {
  if (!passphrase) return null;
  const value = passphrase.trim();
  for (const [network, known] of Object.entries(NETWORK_PASSPHRASES)) {
    if (value === known) return network as StellarNetwork;
  }
  const upper = value.toUpperCase();
  if (upper === "PUBLIC" || upper === "MAINNET") return "PUBLIC";
  if (upper === "TESTNET") return "TESTNET";
  if (upper === "FUTURENET") return "FUTURENET";
  return null;
}

export interface WalletState {
  address: string | null;
  isConnecting: boolean;
  /** Non-null when the last connect() attempt produced an error. */
  connectError: string | null;
  /** Stable code for the last connect() error, if any. */
  connectErrorCode: WalletErrorCode | null;
  /** Network the connected wallet is on, once verified. */
  network: StellarNetwork | null;
  /** Network this app expects; signing is refused unless it matches. */
  expectedNetwork: StellarNetwork;
  connect: () => Promise<void>;
  disconnect: () => void;
}

const WalletContext = createContext<WalletState | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [connectErrorCode, setConnectErrorCode] = useState<WalletErrorCode | null>(null);
  const [network, setNetwork] = useState<StellarNetwork | null>(null);

  const expectedNetwork = useMemo(() => resolveExpectedNetwork(), []);

  const fail = useCallback((code: WalletErrorCode, message: string) => {
    setConnectError(message);
    setConnectErrorCode(code);
    setAddress(null);
    setNetwork(null);
  }, []);

  const connect = useCallback(async () => {
    if (typeof window === "undefined") {
      // Guards against SSR/pre-render invocation; the wallet extension only
      // exists in the browser, so there is nothing to connect to here.
      return;
    }

    // Idempotent connect: ignore re-entrant calls while a connect is in flight
    // so concurrent/replayed requests cannot race the network check.
    if (isConnecting) return;

    setIsConnecting(true);
    setConnectError(null);
    setConnectErrorCode(null);
    try {
      const { isConnected, getAddress, getNetwork } = await import("@stellar/freighter-api");

      let connectedResult: { isConnected: boolean; error?: unknown };
      try {
        connectedResult = await isConnected();
      } catch {
        // isConnected() throws when the extension is not installed
        fail(
          "WALLET_NOT_INSTALLED",
          "Freighter wallet extension is not installed. Install it from freighter.app.",
        );
        return;
      }

      if (connectedResult.error) {
        fail("WALLET_NOT_INSTALLED", "Freighter is not installed. Install it from freighter.app.");
        return;
      }

      if (!connectedResult.isConnected) {
        fail(
          "WALLET_NOT_CONNECTED",
          "Freighter is not connected. Open the extension and unlock your wallet.",
        );
        return;
      }

      let result: { address: string; error?: unknown };
      try {
        result = await getAddress();
      } catch (err) {
        // User rejected the connection request in the extension popup
        const msg = err instanceof Error ? err.message : String(err);
        if (/denied|rejected|cancel/i.test(msg)) {
          fail(
            "WALLET_CONNECTION_REJECTED",
            "Connection request was rejected. Approve it in the Freighter popup.",
          );
        } else {
          fail("WALLET_UNAVAILABLE", `Failed to get wallet address: ${msg}`);
        }
        return;
      }

      if (result.error) {
        const msg = String(result.error);
        if (/denied|rejected|cancel/i.test(msg)) {
          fail(
            "WALLET_CONNECTION_REJECTED",
            "Connection request was rejected. Approve it in the Freighter popup.",
          );
        } else {
          fail("WALLET_UNAVAILABLE", `Failed to get wallet address: ${msg}`);
        }
        return;
      }

      if (!result.address) {
        fail(
          "WALLET_NO_ADDRESS",
          "No address returned from Freighter. Ensure your wallet is unlocked.",
        );
        return;
      }

      // Fail closed: verify the wallet's network passphrase matches the network
      // this app is configured for before accepting the connection.
      let reportedPassphrase: string | null = null;
      try {
        const networkResult = await getNetwork();
        if (networkResult && !networkResult.error) {
          reportedPassphrase = networkResult.networkPassphrase ?? networkResult.network ?? null;
        }
      } catch {
        reportedPassphrase = null;
      }

      const reportedNetwork = resolveNetworkFromPassphrase(reportedPassphrase);
      if (!reportedNetwork) {
        fail(
          "WALLET_NETWORK_UNKNOWN",
          `Could not determine the Freighter network. Expected ${expectedNetwork}. Switch networks in Freighter and retry.`,
        );
        return;
      }

      if (reportedNetwork !== expectedNetwork) {
        fail(
          "WALLET_NETWORK_MISMATCH",
          `Freighter is on ${reportedNetwork} but this app requires ${expectedNetwork}. Switch networks in Freighter and retry.`,
        );
        return;
      }

      setAddress(result.address);
      setNetwork(reportedNetwork);
    } catch (err) {
      // Freighter API not available (extension absent or incompatible)
      const msg = err instanceof Error ? err.message : String(err);
      fail("WALLET_UNAVAILABLE", `Freighter unavailable: ${msg}`);
    } finally {
      setIsConnecting(false);
    }
  }, [expectedNetwork, fail, isConnecting]);

  const disconnect = useCallback(() => {
    setAddress(null);
    setConnectError(null);
    setConnectErrorCode(null);
    setNetwork(null);
  }, []);

  const value = useMemo(
    () => ({
      address,
      isConnecting,
      connectError,
      connectErrorCode,
      network,
      expectedNetwork,
      connect,
      disconnect,
    }),
    [
      address,
      isConnecting,
      connectError,
      connectErrorCode,
      network,
      expectedNetwork,
      connect,
      disconnect,
    ],
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
}

export function useWallet(): WalletState {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return ctx;
}
