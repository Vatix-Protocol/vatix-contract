"use client";

import Link from "next/link";
import { useWallet } from "@/context/WalletContext";

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
          {address ? (
            <button
              type="button"
              onClick={disconnect}
              className="rounded-lg border border-slate-300 px-3 py-1.5 dark:border-slate-600"
              aria-label="Disconnect wallet"
            >
              {truncateAddress(address)}
            </button>
          ) : (
            <button
              type="button"
              disabled={isConnecting}
              onClick={() => void connect()}
              className="rounded-lg bg-indigo-600 px-3 py-1.5 text-white hover:bg-indigo-500 disabled:opacity-60"
              aria-label={isConnecting ? "Connecting wallet" : "Connect wallet"}
            >
              {isConnecting ? "Connecting…" : "Connect wallet"}
            </button>
          )}
        </nav>
      </div>
    </header>
  );
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}
