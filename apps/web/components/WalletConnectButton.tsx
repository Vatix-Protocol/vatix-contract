/**
 * Props for the WalletConnectButton component.
 * @example
 * ```tsx
 * <WalletConnectButton
 *   address={address}
 *   isConnecting={isConnecting}
 *   onConnect={connect}
 *   onDisconnect={disconnect}
 * />
 * ```
 */
export interface WalletConnectButtonProps {
  /** The connected wallet address, or null if not connected. */
  address: string | null;

  /** Whether a wallet connection is in progress. */
  isConnecting: boolean;

  /** Callback function to initiate wallet connection. */
  onConnect: () => void | Promise<void>;

  /** Callback function to disconnect the wallet. */
  onDisconnect: () => void;
}

/**
 * A button component that displays wallet connection status and triggers
 * connect/disconnect actions. Shows "Connect wallet" when disconnected,
 * displays the truncated address when connected, or "Connecting…" during connection.
 */
export function WalletConnectButton({
  address,
  isConnecting,
  onConnect,
  onDisconnect,
}: WalletConnectButtonProps) {
  if (address) {
    return (
      <button
        type="button"
        onClick={onDisconnect}
        className="rounded-lg border border-slate-300 px-3 py-1.5 dark:border-slate-600"
        aria-label="Disconnect wallet"
      >
        {truncateAddress(address)}
      </button>
    );
  }

  return (
    <button
      type="button"
      disabled={isConnecting}
      onClick={() => void onConnect()}
      className="rounded-lg bg-indigo-600 px-3 py-1.5 text-white hover:bg-indigo-500 disabled:opacity-60"
      aria-label={isConnecting ? "Connecting wallet" : "Connect wallet"}
    >
      {isConnecting ? "Connecting…" : "Connect wallet"}
    </button>
  );
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}
