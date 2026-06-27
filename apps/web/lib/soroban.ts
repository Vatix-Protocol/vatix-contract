/**
 * Soroban contract interaction helpers.
 *
 * This module provides helpers for interacting with deployed Soroban contracts
 * using the Stellar SDK and generated TypeScript bindings.
 */

import {
  MARKET_CONTRACT_ID,
  TREASURY_CONTRACT_ID,
  OUTCOME_TOKEN_CONTRACT_ID,
  RESOLUTION_CONTRACT_ID,
} from "./contract-client";

export { MARKET_CONTRACT_ID, TREASURY_CONTRACT_ID, OUTCOME_TOKEN_CONTRACT_ID, RESOLUTION_CONTRACT_ID };

export const SOROBAN_RPC_URL =
  process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ?? "https://soroban-testnet.stellar.org";

export const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ??
  "Test SDF Network ; September 2015";

export const HORIZON_URL =
  process.env.NEXT_PUBLIC_HORIZON_URL ?? "https://horizon-testnet.stellar.org";

// Re-export contract client utilities
export { invokeContract, queryContract } from "./contract-client";
