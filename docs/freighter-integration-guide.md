# Freighter Integration Guide

This guide documents how Vatix integrates with the [Freighter](https://www.freighter.app/) Stellar wallet. It covers the real integration surface used by the web app, the invariants that keep the protocol safe, and the failure modes contributors must handle.

> **Source of truth:** the Vatix server and on-chain contracts are authoritative for balances, swaps, and admin actions. Client-side wallet state (connected address, selected network, cached balances) is **never** authoritative and must not be trusted for policy decisions.

## Integration surface

The web app talks to Freighter through a small, typed surface. The relevant files are:

| Concern | File |
| --- | --- |
| Wallet connect / disconnect, network selection, address retrieval | `apps/web/context/WalletContext.tsx` |
| Connect / disconnect UI | `apps/web/components/WalletConnectButton.tsx` |
| Contract calls (reads and writes) | `apps/web/lib/contract-client.ts` |
| Soroban RPC / transaction assembly | `apps/web/lib/soroban.ts` |

### Connect and disconnect

`WalletContext` exposes `connect()` and `disconnect()`. `connect()` requests access from Freighter, reads the active address, and stores it in context. `disconnect()` clears the local address and any cached state. Neither call mutates server or contract state.

```tsx
const { address, connect, disconnect } = useWallet();

// Connect: prompts Freighter for access and stores the returned address.
await connect();

// Disconnect: clears local wallet state only.
disconnect();
```

### Network selection

Freighter reports the network the user has selected. The app reads this value and compares it against the network the deployment targets. A mismatch is a hard stop for writes — see [Testnet vs mainnet / address drift](#testnet-vs-mainnet--address-drift).

### Address retrieval

`address` is the connected public key. It is used to build transactions and to display the connected account. It is **not** used to authorize anything on the server; the server re-derives identity from the signed transaction.

### Transaction signing

Writes are assembled in `apps/web/lib/soroban.ts`, submitted through `apps/web/lib/contract-client.ts`, and signed by Freighter. The flow is:

1. Build the transaction (contract id, method, args) from server-provided parameters.
2. Ask Freighter to sign the transaction for the connected address.
3. Submit the signed transaction to the Soroban RPC endpoint.
4. Reconcile the result against the server, which remains the source of truth.

```ts
// Simplified: build -> sign with Freighter -> submit -> reconcile.
const tx = await buildTransaction({ contractId, method, args });
const signed = await freighter.signTransaction(tx.toXDR(), { network });
const result = await submitTransaction(signed);
await reconcileWithServer(result);
```

## Invariants

1. **Server/contract is authoritative.** Balances, swaps, and admin state are read from the server or contract, never from wallet context.
2. **Client wallet state is advisory.** A connected address proves nothing until a transaction is signed and verified server-side.
3. **Writes are fail-closed.** If the network, RPC, or server cannot be verified, the write is rejected rather than attempted.
4. **No secrets in the client.** The app never holds signing keys; Freighter holds them.
5. **Every external entrypoint is authorized and rate-limited.** New privileged surfaces are deny-by-default.

## Edge cases and failure modes

### Concurrent / replayed requests (idempotency)

Signing and submission can be retried by the user or the network. Every write carries a client-generated idempotency key (correlation id) so the server can deduplicate replays. Do not assume a single submission per user action.

### Dependency outage (RPC / DB / Redis)

If the Soroban RPC, database, or cache is unavailable, **writes fail closed**. The app surfaces a retryable error and does not optimistically update balances. Reads may fall back to cached values only when explicitly marked stale.

### Auth expiry / wrong role

A signed transaction from an expired session or a wallet without the required role is rejected by the server. The client must handle `401`/`403` by prompting re-authentication, not by retrying blindly.

### Adversarial input and griefing

All inputs (addresses, amounts, contract args) are validated before signing. Malformed or out-of-range values are rejected client-side and re-validated server-side. Rate limits protect entrypoints from griefing.

### Testnet vs mainnet / address drift

Freighter's selected network must match the deployment's expected network. If they differ, writes are blocked and the user is told to switch networks. Contract ids and RPC endpoints are configuration, not hard-coded, so testnet and mainnet addresses cannot drift silently.

## Security considerations

- The server/contract remains the source of truth for balances, swaps, and admin.
- No secrets are stored in the repo or written to logs.
- Every external entrypoint is rate-limited and authorized.
- New privileged surfaces are deny-by-default.
- Money-path changes are feature-flagged or kill-switchable; see the PR description for rollback steps.

## Manual Freighter checklist

- [ ] Connect with Freighter on the expected network; address appears in the UI.
- [ ] Disconnect clears local wallet state.
- [ ] A write on the wrong network is blocked with a clear message.
- [ ] A replayed submission is deduplicated by the server.
- [ ] An RPC outage fails writes closed and surfaces a retryable error.
- [ ] An expired session or wrong role is rejected and prompts re-auth.

## Related docs

- `README.md` — project overview and setup.
- `SECURITY.md` — reporting and security policy.
