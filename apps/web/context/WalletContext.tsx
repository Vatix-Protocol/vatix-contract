"use client";

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Contract, TransactionBuilder, Networks, BASE_FEE, xdr, Address, nativeToScVal } from "@stellar/stellar-sdk";
import { Server } from "@stellar/stellar-sdk/rpc";

export interface WalletState {
  address: string | null;
  isConnecting: boolean;
  connect: () => Promise<void>;
  disconnect: () => void;
  deposit: (amount: string) => Promise<void>;
  withdraw: (amount: string) => Promise<void>;
}

const WalletContext = createContext<WalletState | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);

  const getContractClient = useCallback(() => {
    const contractId = process.env.NEXT_PUBLIC_MARKET_CONTRACT_ID;
    const rpcUrl = process.env.NEXT_PUBLIC_SOROBAN_RPC_URL;
    const networkPassphrase = process.env.NEXT_PUBLIC_SOROBAN_NETWORK_PASSPHRASE;

    if (!contractId || !rpcUrl || !networkPassphrase) {
      throw new Error("Missing contract environment variables");
    }

    const server = new Server(rpcUrl);
    const contract = new Contract(contractId);

    return { server, contract, networkPassphrase };
  }, []);

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
      setAddress("GSTUB…VATIX0000000000000000000000000001");
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => setAddress(null), []);

  const deposit = useCallback(async (amount: string) => {
    if (!address) {
      throw new Error("Wallet not connected");
    }

    const { server, contract, networkPassphrase } = getContractClient();
    const { signTransaction, isConnected } = await import("@stellar/freighter-api");

    if (!(await isConnected())) {
      throw new Error("Freighter not connected");
    }

    const sourceAccount = await server.getAccount(address);
    const amountVal = nativeToScVal(BigInt(amount), { type: "i128" });
    
    const tx = new TransactionBuilder(sourceAccount, {
      fee: BASE_FEE,
      networkPassphrase,
    })
      .addOperation(contract.call("deposit", amountVal))
      .setTimeout(30)
      .build();

    const signedTxResult = await signTransaction(tx.toXDR(), {
      networkPassphrase,
    });

    const submitResponse = await server.sendTransaction(
      TransactionBuilder.fromXDR(signedTxResult.signedTxXdr, networkPassphrase),
    );

    if (submitResponse.status !== "PENDING") {
      throw new Error(`Transaction failed: ${JSON.stringify(submitResponse)}`);
    }

    let getTxResponse = await server.getTransaction(submitResponse.hash);
    while (getTxResponse.status === "NOT_FOUND") {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      getTxResponse = await server.getTransaction(submitResponse.hash);
    }

    if (getTxResponse.status !== "SUCCESS") {
      throw new Error(`Transaction failed: ${JSON.stringify(getTxResponse)}`);
    }
  }, [address, getContractClient]);

  const withdraw = useCallback(async (amount: string) => {
    if (!address) {
      throw new Error("Wallet not connected");
    }

    const { server, contract, networkPassphrase } = getContractClient();
    const { signTransaction, isConnected } = await import("@stellar/freighter-api");

    if (!(await isConnected())) {
      throw new Error("Freighter not connected");
    }

    const sourceAccount = await server.getAccount(address);
    const amountVal = nativeToScVal(BigInt(amount), { type: "i128" });
    
    const tx = new TransactionBuilder(sourceAccount, {
      fee: BASE_FEE,
      networkPassphrase,
    })
      .addOperation(contract.call("withdraw", amountVal))
      .setTimeout(30)
      .build();

    const signedTxResult = await signTransaction(tx.toXDR(), {
      networkPassphrase,
    });

    const submitResponse = await server.sendTransaction(
      TransactionBuilder.fromXDR(signedTxResult.signedTxXdr, networkPassphrase),
    );

    if (submitResponse.status !== "PENDING") {
      throw new Error(`Transaction failed: ${JSON.stringify(submitResponse)}`);
    }

    let getTxResponse = await server.getTransaction(submitResponse.hash);
    while (getTxResponse.status === "NOT_FOUND") {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      getTxResponse = await server.getTransaction(submitResponse.hash);
    }

    if (getTxResponse.status !== "SUCCESS") {
      throw new Error(`Transaction failed: ${JSON.stringify(getTxResponse)}`);
    }
  }, [address, getContractClient]);

  const value = useMemo(
    () => ({ address, isConnecting, connect, disconnect, deposit, withdraw }),
    [address, isConnecting, connect, disconnect, deposit, withdraw],
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
}

export function useWallet(): WalletState {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return ctx;
}
