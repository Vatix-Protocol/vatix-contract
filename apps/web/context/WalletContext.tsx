"use client";

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export interface WalletState {
  address: string | null;
  isConnecting: boolean;
  /** Non-null when the last connect() attempt produced an error. */
  connectError: string | null;
  connect: () => Promise<void>;
  disconnect: () => void;
}

const WalletContext = createContext<WalletState | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);

  const connect = useCallback(async () => {
    setIsConnecting(true);
    setConnectError(null);
    try {
      const { isConnected, getAddress } = await import("@stellar/freighter-api");

      let connectedResult: { isConnected: boolean; error?: unknown };
      try {
        connectedResult = await isConnected();
      } catch {
        // isConnected() throws when the extension is not installed
        setConnectError("Freighter wallet extension is not installed. Install it from freighter.app.");
        return;
      }

      if (connectedResult.error) {
        setConnectError("Freighter is not installed. Install it from freighter.app.");
        setAddress(null);
        return;
      }

      if (!connectedResult.isConnected) {
        setConnectError("Freighter is not connected. Open the extension and unlock your wallet.");
        setAddress(null);
        return;
      }

      let result: { address: string; error?: unknown };
      try {
        result = await getAddress();
      } catch (err) {
        // User rejected the connection request in the extension popup
        const msg = err instanceof Error ? err.message : String(err);
        if (/denied|rejected|cancel/i.test(msg)) {
          setConnectError("Connection request was rejected. Approve it in the Freighter popup.");
        } else {
          setConnectError(`Failed to get wallet address: ${msg}`);
        }
        return;
      }

      if (result.error) {
        const msg = String(result.error);
        if (/denied|rejected|cancel/i.test(msg)) {
          setConnectError("Connection request was rejected. Approve it in the Freighter popup.");
        } else {
          setConnectError(`Failed to get wallet address: ${msg}`);
        }
        return;
      }

      if (!result.address) {
        setConnectError("No address returned from Freighter. Ensure your wallet is unlocked.");
        return;
      }

      setAddress(result.address);
    } catch (err) {
      // Freighter API not available (extension absent or incompatible)
      const msg = err instanceof Error ? err.message : String(err);
      setConnectError(`Freighter unavailable: ${msg}`);
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => {
    setAddress(null);
    setConnectError(null);
  }, []);

  const value = useMemo(
    () => ({ address, isConnecting, connectError, connect, disconnect }),
    [address, isConnecting, connectError, connect, disconnect],
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
