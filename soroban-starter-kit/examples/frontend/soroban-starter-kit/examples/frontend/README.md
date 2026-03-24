# Frontend Integration Example

A web frontend example demonstrating how to integrate with the Soroban Token and Escrow contracts using the Freighter wallet.

## Prerequisites

- Node.js 16+
- [Freighter wallet](https://freighter.app/) browser extension
- Deployed Token and/or Escrow contract IDs (see `contracts/`)

## Quick Start

```bash
cd examples/frontend
npm install
npm run dev
```

## Wallet Integration

This example uses [Freighter](https://freighter.app/) for wallet connectivity.

```typescript
import { isConnected, getPublicKey, signTransaction } from '@stellar/freighter-api';

// Check if Freighter is installed
const connected = await isConnected();

// Get the user's public key
const publicKey = await getPublicKey();
```

## Contract Interaction

### Setup

```typescript
import { SorobanRpc, TransactionBuilder, Networks, Contract, nativeToScVal } from '@stellar/stellar-sdk';

const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
const networkPassphrase = Networks.TESTNET;
```

### Calling a Contract Function

```typescript
async function callContract(contractId: string, method: string, params: xdr.ScVal[]) {
  const publicKey = await getPublicKey();
  const account = await server.getAccount(publicKey);
  const contract = new Contract(contractId);

  const transaction = new TransactionBuilder(account, {
    fee: '100',
    networkPassphrase,
  })
    .addOperation(contract.call(method, ...params))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(transaction);
  const signed = await signTransaction(prepared.toXDR(), { network: 'TESTNET' });

  const result = await server.sendTransaction(
    TransactionBuilder.fromXDR(signed, networkPassphrase)
  );
  return result;
}
```

## Token Contract

```typescript
const TOKEN_CONTRACT_ID = '<YOUR_TOKEN_CONTRACT_ID>';

// Check balance
await callContract(TOKEN_CONTRACT_ID, 'balance', [
  nativeToScVal(publicKey, { type: 'address' }),
]);

// Transfer tokens
await callContract(TOKEN_CONTRACT_ID, 'transfer', [
  nativeToScVal(fromAddress, { type: 'address' }),
  nativeToScVal(toAddress, { type: 'address' }),
  nativeToScVal(amount, { type: 'i128' }),
]);
```

## Escrow Contract

```typescript
const ESCROW_CONTRACT_ID = '<YOUR_ESCROW_CONTRACT_ID>';

// Fund escrow (buyer)
await callContract(ESCROW_CONTRACT_ID, 'fund', []);

// Mark as delivered (seller)
await callContract(ESCROW_CONTRACT_ID, 'mark_delivered', []);

// Approve delivery and release funds (buyer)
await callContract(ESCROW_CONTRACT_ID, 'approve_delivery', []);
```

## Resources

- [Stellar SDK Docs](https://stellar.github.io/js-stellar-sdk/)
- [Soroban Docs](https://soroban.stellar.org/docs)
- [Freighter API](https://docs.freighter.app/)
