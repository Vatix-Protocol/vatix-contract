"use client";

import Link from "next/link";
import { useWallet } from "@/context/WalletContext";
import { WalletConnectButton } from "./WalletConnectButton";

/**
 * Navigation bar component for the Vatix prediction market application.
 * 
 * Displays the site logo, navigation links, and wallet connection status.
 * When the user has no wallet connected, shows a friendly prompt to connect.
 */
export function Navbar() {
  const { address, isConnecting, connectError, connect, disconnect } = useWallet();

  return (
    <header className="border-b border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950">
      <div className="mx-auto flex max-w-4xl items-center justify-between px-4 py-4">
        <Link href="/" className="font-semibold text-indigo-600 dark:text-indigo-300">
          Vatix
        </Link>
        <nav className="flex items-center gap-6 text-sm text-slate-700 dark:text-slate-200">
          <Link href="/markets" className="hover:text-indigo-600 dark:hover:text-indigo-300">
            Markets
          </Link>
          {!address && !isConnecting && (
            <span className="text-xs text-slate-500 dark:text-slate-400">
              Connect your wallet to trade →
            </span>
          )}
          <WalletConnectButton
            address={address}
            isConnecting={isConnecting}
            connectError={connectError}
            onConnect={connect}
            onDisconnect={disconnect}
          />
        </nav>
      </div>
    </header>
  );
}
