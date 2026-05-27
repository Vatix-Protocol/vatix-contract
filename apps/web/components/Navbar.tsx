"use client";

import Link from "next/link";
import { useWallet } from "@/context/WalletContext";
import { WalletConnectButton } from "./WalletConnectButton";

export function Navbar() {
  const { address, isConnecting, connect, disconnect } = useWallet();

  return (
    <header className="border-b border-slate-200 dark:border-slate-800">
      <div className="mx-auto flex max-w-4xl items-center justify-between px-4 py-4">
        <Link href="/" className="font-semibold text-indigo-600 dark:text-indigo-400">
          Vatix
        </Link>
        <nav className="flex items-center gap-6 text-sm">
          <Link href="/markets" className="hover:text-indigo-600">
            Markets
          </Link>
          <WalletConnectButton
            address={address}
            isConnecting={isConnecting}
            onConnect={connect}
            onDisconnect={disconnect}
          />
        </nav>
      </div>
    </header>
  );
}
