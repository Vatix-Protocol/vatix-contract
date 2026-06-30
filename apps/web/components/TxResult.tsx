"use client";

/**
 * Displays a confirmed transaction hash with a link to the Stellar explorer.
 *
 * The explorer base URL defaults to Stellar Expert (testnet) and can be
 * overridden via NEXT_PUBLIC_EXPLORER_URL.
 */

const EXPLORER_BASE =
  process.env.NEXT_PUBLIC_EXPLORER_URL ??
  "https://stellar.expert/explorer/testnet/tx";

interface TxResultProps {
  /** Full transaction hash returned by the Soroban RPC. */
  hash: string;
  /** Optional label shown before the link (defaults to "Transaction"). */
  label?: string;
}

export function TxResult({ hash, label = "Transaction" }: TxResultProps) {
  const explorerUrl = `${EXPLORER_BASE}/${hash}`;
  const short = `${hash.slice(0, 8)}…${hash.slice(-8)}`;

  return (
    <p
      role="status"
      className="text-sm text-green-600 dark:text-green-400 break-all"
    >
      {label} confirmed:{" "}
      <a
        href={explorerUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="underline hover:text-green-500"
        aria-label={`View transaction ${hash} on Stellar explorer`}
      >
        {short}
      </a>
    </p>
  );
}
