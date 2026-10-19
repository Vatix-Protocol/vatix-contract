"use client";

/**
 * Displays a confirmed transaction hash with a link to the Stellar explorer.
 *
 * The explorer base URL defaults to Stellar Expert (testnet) and can be
 * overridden via NEXT_PUBLIC_EXPLORER_URL.
 */

/** Default explorer base URL used when `NEXT_PUBLIC_EXPLORER_URL` is unset. */
export const DEFAULT_EXPLORER_BASE = "https://stellar.expert/explorer/testnet/tx";

/**
 * A Stellar transaction hash is the 32-byte hex SHA-256 of the transaction
 * envelope, rendered as 64 hexadecimal characters. Anything else is not a
 * plausible hash and must not be turned into a clickable link.
 */
const TX_HASH_PATTERN = /^[0-9a-fA-F]{64}$/;

/**
 * Strip trailing slashes from a base URL so joining it with a path segment
 * never produces a double slash (`.../tx//<hash>`).
 */
export function normalizeExplorerBase(base: string): string {
  return base.replace(/\/+$/, "");
}

/**
 * True when `hash` looks like a canonical Stellar transaction hash. Empty or
 * malformed hashes are rejected so the UI can fall back to plain text instead
 * of rendering a link to the explorer root.
 */
export function isPlausibleTxHash(hash: string): boolean {
  return TX_HASH_PATTERN.test(hash.trim());
}

/**
 * Build the explorer URL for a transaction hash, or `null` when the hash is
 * not a plausible transaction hash (in which case no link should be shown).
 */
export function buildExplorerTxUrl(
  base: string,
  hash: string,
): string | null {
  const trimmed = hash.trim();
  if (!isPlausibleTxHash(trimmed)) {
    return null;
  }
  return `${normalizeExplorerBase(base)}/${trimmed}`;
}

// Normalize once at module scope so every render joins the same way.
const EXPLORER_BASE = normalizeExplorerBase(
  process.env.NEXT_PUBLIC_EXPLORER_URL ?? DEFAULT_EXPLORER_BASE,
);

interface TxResultProps {
  /** Full transaction hash returned by the Soroban RPC. */
  hash: string;
  /** Optional label shown before the link (defaults to "Transaction"). */
  label?: string;
}

export function TxResult({ hash, label = "Transaction" }: TxResultProps) {
  const explorerUrl = buildExplorerTxUrl(EXPLORER_BASE, hash);
  const trimmed = hash.trim();
  const short = `${trimmed.slice(0, 8)}…${trimmed.slice(-8)}`;

  return (
    <p
      role="status"
      className="text-sm text-green-600 dark:text-green-400 break-all"
    >
      {label} confirmed:{" "}
      {explorerUrl ? (
        <a
          href={explorerUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="underline hover:text-green-500"
          aria-label={`View transaction ${trimmed} on Stellar explorer`}
        >
          {short}
        </a>
      ) : (
        // No plausible hash: show plain text rather than a misleading link to
        // the explorer root that would look like a successful confirmation.
        <span data-testid="tx-hash-plain">{trimmed || "unknown"}</span>
      )}
    </p>
  );
}
