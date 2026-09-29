import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * Freighter network passphrase handling.
 *
 * The Stellar network passphrase is the source of truth for which network a
 * signed transaction targets. We resolve the expected passphrase from the
 * configured environment and fail closed whenever the connected Freighter
 * wallet reports a different network.
 */

// Stable error codes surfaced to callers/telemetry. Never include secrets.
export const FreighterNetworkError = {
  MISSING_CONFIG: 'FREIGHTER_MISSING_NETWORK_PASSPHRASE',
  UNSUPPORTED_NETWORK: 'FREIGHTER_UNSUPPORTED_NETWORK',
  NETWORK_MISMATCH: 'FREIGHTER_NETWORK_MISMATCH',
  CONNECT_FAILED: 'FREIGHTER_CONNECT_FAILED',
} as const;

export type FreighterNetworkErrorCode =
  (typeof FreighterNetworkError)[keyof typeof FreighterNetworkError];

// Canonical Stellar network passphrases.
export const STELLAR_NETWORK_PASSPHRASES = {
  PUBLIC: 'Public Global Stellar Network ; September 2015',
  TESTNET: 'Test SDF Network ; September 2015',
} as const;

export type StellarNetwork = keyof typeof STELLAR_NETWORK_PASSPHRASES;

/**
 * Resolve the expected network passphrase from configuration.
 * Fails closed (returns an error) when the config is missing or unknown.
 */
export function resolveExpectedPassphrase(
  configured?: string | null,
): { ok: true; passphrase: string; network: StellarNetwork } | { ok: false; code: FreighterNetworkErrorCode } {
  const raw = (configured ?? '').trim();
  if (!raw) {
    return { ok: false, code: FreighterNetworkError.MISSING_CONFIG };
  }

  const normalized = raw.toUpperCase();
  if (normalized === 'PUBLIC' || normalized === 'MAINNET') {
    return { ok: true, passphrase: STELLAR_NETWORK_PASSPHRASES.PUBLIC, network: 'PUBLIC' };
  }
  if (normalized === 'TESTNET') {
    return { ok: true, passphrase: STELLAR_NETWORK_PASSPHRASES.TESTNET, network: 'TESTNET' };
  }

  // Allow an explicit passphrase string as well.
  const known = Object.values(STELLAR_NETWORK_PASSPHRASES) as string[];
  if (known.includes(raw)) {
    const network = (Object.keys(STELLAR_NETWORK_PASSPHRASES) as StellarNetwork[]).find(
      (key) => STELLAR_NETWORK_PASSPHRASES[key] === raw,
    ) as StellarNetwork;
    return { ok: true, passphrase: raw, network };
  }

  return { ok: false, code: FreighterNetworkError.UNSUPPORTED_NETWORK };
}

/**
 * Validate that the connected wallet's network passphrase matches the expected
 * one. Deny-by-default: any mismatch or missing value is rejected.
 */
export function validateNetworkPassphrase(
  expected: string,
  actual?: string | null,
): { ok: true } | { ok: false; code: FreighterNetworkErrorCode } {
  if (!actual || actual.trim() !== expected) {
    return { ok: false, code: FreighterNetworkError.NETWORK_MISMATCH };
  }
  return { ok: true };
}

interface FreighterApi {
  isConnected: () => Promise<{ isConnected: boolean }>;
  requestAccess: () => Promise<{ address: string }>;
  getNetwork: () => Promise<{ network: string; networkPassphrase: string }>;
  getPublicKey: () => Promise<{ publicKey: string }>;
}

interface WalletConnectButtonProps {
  /** Configured expected network (e.g. "TESTNET" or "PUBLIC"). */
  expectedNetwork?: string;
  onConnected?: (address: string) => void;
  onError?: (code: FreighterNetworkErrorCode) => void;
}

const EXPECTED_NETWORK =
  (typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_STELLAR_NETWORK) || 'TESTNET';

export default function WalletConnectButton({
  expectedNetwork = EXPECTED_NETWORK,
  onConnected,
  onError,
}: WalletConnectButtonProps) {
  const [address, setAddress] = useState<string | null>(null);
  const [error, setError] = useState<FreighterNetworkErrorCode | null>(null);
  const [connecting, setConnecting] = useState(false);
  // Guards against concurrent/replayed connect attempts (idempotent connect).
  const inFlight = useRef(false);

  const fail = useCallback(
    (code: FreighterNetworkErrorCode) => {
      setError(code);
      onError?.(code);
    },
    [onError],
  );

  const connect = useCallback(async () => {
    if (inFlight.current) {
      return;
    }

    const resolved = resolveExpectedPassphrase(expectedNetwork);
    if (!resolved.ok) {
      fail(resolved.code);
      return;
    }

    inFlight.current = true;
    setConnecting(true);
    setError(null);

    try {
      const freighter = (window as unknown as { freighter?: FreighterApi }).freighter;
      if (!freighter) {
        fail(FreighterNetworkError.CONNECT_FAILED);
        return;
      }

      const { isConnected } = await freighter.isConnected();
      if (!isConnected) {
        await freighter.requestAccess();
      }

      // Fail closed: verify the wallet network before trusting the address.
      const { networkPassphrase } = await freighter.getNetwork();
      const check = validateNetworkPassphrase(resolved.passphrase, networkPassphrase);
      if (!check.ok) {
        fail(check.code);
        return;
      }

      const { publicKey } = await freighter.getPublicKey();
      setAddress(publicKey);
      onConnected?.(publicKey);
    } catch {
      // Never log raw wallet payloads; surface a stable code only.
      fail(FreighterNetworkError.CONNECT_FAILED);
    } finally {
      inFlight.current = false;
      setConnecting(false);
    }
  }, [expectedNetwork, fail, onConnected]);

  // Re-validate on network change so address drift is caught without a reload.
  useEffect(() => {
    const freighter = (window as unknown as { freighter?: FreighterApi }).freighter;
    if (!freighter || !address) {
      return;
    }

    const resolved = resolveExpectedPassphrase(expectedNetwork);
    if (!resolved.ok) {
      fail(resolved.code);
      return;
    }

    let cancelled = false;
    freighter
      .getNetwork()
      .then(({ networkPassphrase }) => {
        if (cancelled) {
          return;
        }
        const check = validateNetworkPassphrase(resolved.passphrase, networkPassphrase);
        if (!check.ok) {
          setAddress(null);
          fail(check.code);
        }
      })
      .catch(() => {
        if (!cancelled) {
          fail(FreighterNetworkError.CONNECT_FAILED);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [address, expectedNetwork, fail]);

  return (
    <div>
      <button type="button" onClick={connect} disabled={connecting || !!address}>
        {address ? `Connected: ${address.slice(0, 4)}…${address.slice(-4)}` : connecting ? 'Connecting…' : 'Connect Freighter'}
      </button>
      {error && (
        <p role="alert" data-error-code={error}>
          {error === FreighterNetworkError.NETWORK_MISMATCH
            ? 'Freighter is on the wrong network. Switch networks and reconnect.'
            : 'Unable to connect Freighter. Please try again.'}
        </p>
      )}
    </div>
  );
}
