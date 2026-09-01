/**
 * Props for the WalletConnectButton component.
 * @example
 * ```tsx
 * <WalletConnectButton
 *   address={address}
 *   isConnecting={isConnecting}
 *   connectError={connectError}
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

  /** Error message from the last failed connect() attempt, or null. */
  connectError?: string | null;

  /** Callback function to initiate wallet connection. */
  onConnect: () => void | Promise<void>;

  /** Callback function to disconnect the wallet. */
  onDisconnect: () => void;
}

/**
 * A button component that displays wallet connection status and triggers
 * connect/disconnect actions. Shows an inline error message below the button
 * when a connection attempt fails (e.g., extension not installed, user rejected).
 */
export function WalletConnectButton({
  address,
  isConnecting,
  connectError,
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
    <div className="flex flex-col items-start gap-1">
      <button
        type="button"
        disabled={isConnecting}
        onClick={() => void onConnect()}
        className="rounded-lg bg-indigo-600 px-3 py-1.5 text-white hover:bg-indigo-500 disabled:opacity-60"
        aria-label={isConnecting ? "Connecting wallet" : "Connect wallet"}
      >
        {isConnecting ? "Connecting…" : "Connect wallet"}
      </button>
      {connectError && (
        <p
          role="alert"
          className="max-w-xs text-xs text-red-600 dark:text-red-400"
        >
          {connectError}
        </p>
      )}
    </div>
  );
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}
