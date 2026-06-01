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
  connect: () => Promise<void>;
  disconnect: () => void;
}

const WalletContext = createContext<WalletState | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);

  const connect = useCallback(async () => {
    setIsConnecting(true);
    try {
      const { isConnected, getAddress } = await import("@stellar/freighter-api");
      if (!(await isConnected())) {
        setAddress(null);
        return;
      }
      const result = await getAddress();
      if (result.address) {
        setAddress(result.address);
      }
    } catch {
      // Freighter not installed — stub address for local UI work
      setAddress("GSTUB…VATIX00000000000000000000000000001");
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => setAddress(null), []);

  const value = useMemo(
    () => ({ address, isConnecting, connect, disconnect }),
    [address, isConnecting, connect, disconnect],
  );

  const statusMessage = isConnecting
    ? "Connecting wallet…"
    : address
      ? `Wallet connected: ${address}`
      : "Wallet disconnected";

  return (
    <WalletContext.Provider value={value}>
      {children}
      {/*
       * Screen-reader live region: announces wallet connection state changes
       * (connecting, connected, disconnected) to assistive technology without
       * requiring focus. aria-label gives the region a descriptive accessible
       * name so screen readers identify it as "Wallet status".
       */}
      <span
        role="status"
        aria-live="polite"
        aria-label="Wallet status"
        aria-atomic="true"
        className="sr-only"
      >
        {statusMessage}
      </span>
    </WalletContext.Provider>
  );
}

export function useWallet(): WalletState {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return ctx;
}
