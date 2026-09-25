# Integration Guide

Step-by-step instructions for integrating Soroban contracts into your application.

---

## 1. Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Node.js | 20+ | https://nodejs.org |
| Rust | 1.78+ | https://rustup.rs |
| Stellar CLI | latest | `cargo install --locked stellar-cli --features opt` |
| Freighter Wallet | latest | https://freighter.app |

---

## 2. Install the SDK

```bash
npm install @stellar/stellar-sdk
```

---

## 3. Connect to a Network

```ts
import { SorobanRpc, Networks } from '@stellar/stellar-sdk';

// Testnet
const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
const networkPassphrase = Networks.TESTNET;

// Mainnet
// const server = new SorobanRpc.Server('https://soroban.stellar.org');
// const networkPassphrase = Networks.PUBLIC;
```

---

## 4. Load a Deployed Contract

```ts
import { Contract } from '@stellar/stellar-sdk';

const contract = new Contract('<CONTRACT_ID>');
```

Replace `<CONTRACT_ID>` with the address returned by `./scripts/deploy.sh`.

---

## 5. Invoke a Contract Function

```ts
import { TransactionBuilder, BASE_FEE, Keypair, xdr } from '@stellar/stellar-sdk';

const sourceKeypair = Keypair.fromSecret('<SECRET_KEY>');
const account = await server.getAccount(sourceKeypair.publicKey());

const tx = new TransactionBuilder(account, {
  fee: BASE_FEE,
  networkPassphrase,
})
  .addOperation(contract.call('balance', xdr.ScVal.scvAddress(/* address */)))
  .setTimeout(30)
  .build();

const prepared = await server.prepareTransaction(tx);
prepared.sign(sourceKeypair);

const result = await server.sendTransaction(prepared);
```

---

## 6. Token Contract Integration

### Initialize

```ts
contract.call('initialize',
  adminAddress,   // xdr.ScVal address
  nameScVal,      // xdr.ScVal string
  symbolScVal,    // xdr.ScVal string
  decimalsScVal,  // xdr.ScVal u32
)
```

### Mint / Transfer / Burn

```ts
contract.call('mint', toAddress, amountScVal);
contract.call('transfer', fromAddress, toAddress, amountScVal);
contract.call('burn', fromAddress, amountScVal);
```

---

## 7. Escrow Contract Integration

```ts
// Create escrow
contract.call('create', buyerAddress, sellerAddress, tokenAddress, amountScVal, deadlineScVal);

// Release funds (seller)
contract.call('release', escrowIdScVal);

// Refund after deadline (buyer)
contract.call('refund', escrowIdScVal);
```

---

## 8. Events Reference

This section documents the exact XDR types for every event emitted by the token and escrow contracts. Indexers must decode topics and data from on-chain XDR to reconstruct event payloads.

### Type Encoding Reference

| Soroban Type | XDR ScVal Variant | Notes |
|---|---|---|
| `Symbol` | `ScvSymbol` | Up to 32 ASCII chars |
| `Address` | `ScvAddress` → `ScvAccountId` or `ScvContractId` | 32-byte public key or contract hash |
| `i128` | `ScvI128` → `{ hi: i64, lo: u64 }` | 128-bit signed integer |
| `u32` | `ScvU32` | 32-bit unsigned integer |
| `BytesN<32>` | `ScvBytes` | Fixed 32-byte array (WASM hash) |
| `String` | `ScvString` | UTF-8 bytes |
| `()` (unit) | `ScvVoid` | Empty data payload |
| Tuple `(A, B)` | `ScvVec([A, B])` | Ordered vector of ScVals |

### Token Contract — Event Details

#### `initialized`

Emitted once when the token is first configured.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"initialized"` | topic[0] | `ScvSymbol` | Event name |
| admin | topic[1] | `ScvAddress` | Account that initialized the contract |
| name | data[0] | `ScvString` | Token name (e.g. `"MyToken"`) |
| symbol | data[1] | `ScvString` | Token ticker symbol (e.g. `"MTK"`) |
| decimals | data[2] | `ScvU32` | Number of decimal places |

Example (testnet, base64 XDR — illustrative):
```
topic[0]: AAAABS5pbml0aWFsaXplZA==   # ScvSymbol("initialized")
topic[1]: AAAAA...                    # ScvAddress(admin_pubkey)
data:     AAAAEQAAAAcAAAABTXlUb2tlbg== # ScvVec(["MyToken", "MTK", 7])
```

---

#### `mint`

Emitted when new tokens are created and assigned to a recipient.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"mint"` | topic[0] | `ScvSymbol` | Event name |
| to | topic[1] | `ScvAddress` | Recipient address |
| amount | data | `ScvI128` | Tokens minted in base units |

Example:
```
topic[0]: AAAABS5taW50          # ScvSymbol("mint")
topic[1]: AAAAA...              # ScvAddress(recipient)
data:     AAAADgAAAAAAAAAAAAAAAABiO8A=  # ScvI128(10_000_000, at 7 decimals = 1 token)
```

---

#### `burn`

Emitted when tokens are removed from circulation.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"burn"` | topic[0] | `ScvSymbol` | Event name |
| from | topic[1] | `ScvAddress` | Account whose tokens are burned |
| amount | data | `ScvI128` | Tokens burned in base units |

---

#### `transfer`

Emitted on every direct token transfer.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"transfer"` | topic[0] | `ScvSymbol` | Event name |
| from | topic[1] | `ScvAddress` | Sender address |
| to | topic[2] | `ScvAddress` | Receiver address |
| amount | data | `ScvI128` | Transferred amount in base units |

---

#### `approve`

Emitted when an owner grants a spender an allowance.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"approve"` | topic[0] | `ScvSymbol` | Event name |
| from | topic[1] | `ScvAddress` | Owner granting the allowance |
| spender | topic[2] | `ScvAddress` | Account receiving the allowance |
| amount | data | `ScvI128` | Allowance amount in base units |

---

#### `revoke`

Emitted when an existing allowance is revoked.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"revoke"` | topic[0] | `ScvSymbol` | Event name |
| from | topic[1] | `ScvAddress` | Owner revoking the allowance |
| spender | topic[2] | `ScvAddress` | Account losing the allowance |
| (void) | data | `ScvVoid` | No data payload |

---

#### `admin_changed`

Emitted immediately when the admin key is rotated.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"admin_changed"` | topic[0] | `ScvSymbol` | Event name |
| old_admin | topic[1] | `ScvAddress` | Previous admin address |
| new_admin | data | `ScvAddress` | Newly assigned admin address |

---

#### `admin_proposed`

Emitted when a new admin candidate is nominated via the two-step rotation flow.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"admin_proposed"` | topic[0] | `ScvSymbol` | Event name |
| current_admin | topic[1] | `ScvAddress` | Current admin proposing the change |
| pending_admin | data | `ScvAddress` | Nominated candidate |

---

#### `admin_accepted`

Emitted when the pending admin accepts the role.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"admin_accepted"` | topic[0] | `ScvSymbol` | Event name |
| new_admin | topic[1] | `ScvAddress` | Address that accepted the admin role |
| (void) | data | `ScvVoid` | No data payload |

---

#### `paused` / `unpaused`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"paused"` or `"unpaused"` | topic[0] | `ScvSymbol` | Event name |
| admin | topic[1] | `ScvAddress` | Admin that toggled the pause |
| (void) | data | `ScvVoid` | No data payload |

---

#### `upgraded`

Emitted after a successful WASM upgrade.

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"upgraded"` | topic[0] | `ScvSymbol` | Event name |
| admin | topic[1] | `ScvAddress` | Admin that executed the upgrade |
| wasm_hash | data | `ScvBytes` (32 bytes) | Hash of the new WASM module |

---

### Escrow Contract — Event Details

#### `initialized`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"initialized"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer address |
| seller | topic[2] | `ScvAddress` | Seller address |
| arbiter | topic[3] | `ScvAddress` | Arbiter address |
| amount | data | `ScvI128` | Escrowed amount in base units |

---

#### `escrow_created`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"escrow_created"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer address |
| seller | topic[2] | `ScvAddress` | Seller address |
| amount | data | `ScvI128` | Agreed amount in base units |

---

#### `escrow_funded`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"escrow_funded"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer who funded |
| amount | data | `ScvI128` | Amount transferred into escrow |

---

#### `delivery_marked`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"delivery_marked"` | topic[0] | `ScvSymbol` | Event name |
| seller | topic[1] | `ScvAddress` | Seller who marked delivery |
| (void) | data | `ScvVoid` | No data payload |

---

#### `funds_released`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"funds_released"` | topic[0] | `ScvSymbol` | Event name |
| seller | topic[1] | `ScvAddress` | Seller who received the funds |
| amount | data | `ScvI128` | Amount released in base units |

---

#### `partial_release`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"partial_release"` | topic[0] | `ScvSymbol` | Event name |
| seller | topic[1] | `ScvAddress` | Seller who received the partial payment |
| amount | data | `ScvI128` | Partial amount released in base units |

---

#### `funds_refunded`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"funds_refunded"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer who received the refund |
| amount | data | `ScvI128` | Refunded amount in base units |

---

#### `dispute_raised`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"dispute_raised"` | topic[0] | `ScvSymbol` | Event name |
| caller | topic[1] | `ScvAddress` | Party (buyer or seller) who raised the dispute |
| (void) | data | `ScvVoid` | No data payload |

---

#### `amount_updated`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"amount_updated"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer who changed the amount |
| new_amount | data | `ScvI128` | Updated escrow amount in base units |

---

#### `deadline_extended`

| Field | Position | XDR Type | Meaning |
|---|---|---|---|
| `"deadline_extended"` | topic[0] | `ScvSymbol` | Event name |
| buyer | topic[1] | `ScvAddress` | Buyer who requested the extension |
| new_deadline | data | `ScvU32` | New deadline ledger sequence number |

---

### Decoding with the Stellar CLI

```bash
# Decode a single ScVal from base64 XDR
stellar xdr decode --type ScVal --xdr "<BASE64>"

# Fetch the last 10 events for a contract as JSON
stellar contract events \
  --id <CONTRACT_ID> \
  --network testnet \
  --output json \
  | jq '.[] | {name: .topic[0], data: .value}'
```

---

## 8a. Event Topic Convention

All events in both Token and Escrow contracts follow a standardized topic structure for consistent indexing:

### Topic Structure

Events are published with topics in the following format:
- **Topic 0**: Event name (Symbol)
- **Topic 1**: Primary actor (Address) — typically the initiator or affected party
- **Topic 2** (optional): Secondary actor (Address) — for two-party operations
- **Topic 3** (optional): Tertiary actor (Address) — for three-party operations

### Token Contract Events

| Event | Topics | Data |
|-------|--------|------|
| `initialized` | `(Symbol, Address)` | `(name, symbol, decimals)` |
| `mint` | `(Symbol, Address)` | `amount` |
| `burn` | `(Symbol, Address)` | `amount` |
| `transfer` | `(Symbol, Address, Address)` | `amount` |
| `approve` | `(Symbol, Address, Address)` | `amount` |
| `revoke` | `(Symbol, Address, Address)` | `()` |
| `admin_changed` | `(Symbol, Address)` | `new_admin` |
| `paused` | `(Symbol, Address)` | `()` |
| `unpaused` | `(Symbol, Address)` | `()` |
| `upgraded` | `(Symbol, Address)` | `wasm_hash` |

### Escrow Contract Events

| Event | Topics | Data |
|-------|--------|------|
| `initialized` | `(Symbol, Address, Address, Address)` | `amount` |
| `escrow_created` | `(Symbol, Address, Address)` | `amount` |
| `escrow_funded` | `(Symbol, Address)` | `amount` |
| `delivery_marked` | `(Symbol, Address)` | `()` |
| `funds_released` | `(Symbol, Address)` | `amount` |
| `funds_refunded` | `(Symbol, Address)` | `amount` |
| `dispute_raised` | `(Symbol, Address)` | `()` |
| `paused` | `(Symbol, Address)` | `()` |
| `unpaused` | `(Symbol, Address)` | `()` |
| `upgraded` | `(Symbol, Address)` | `wasm_hash` |

### Indexing Events

When indexing events, filter by topic structure:

```ts
// Token: Get all transfers from an address
const transfers = events.filter(e => 
  e.topics[0] === 'transfer' && 
  e.topics[1] === senderAddress
);

// Escrow: Get all escrows created by a buyer
const escrows = events.filter(e => 
  e.topics[0] === 'escrow_created' && 
  e.topics[1] === buyerAddress
);
```

---

## 8b. Subscribing to Contract Events via RPC

The Stellar RPC exposes a `getEvents` endpoint that lets you query and poll for
contract events. This section shows how to subscribe to escrow and token events
from TypeScript using `@stellar/stellar-sdk`.

### How `getEvents` Works

`getEvents` returns events emitted within a ledger range. It accepts:

- `startLedger` — the first ledger to include (required on first call)
- `filters` — one or more filter objects, each specifying:
  - `type` — `"contract"` for `env.events().publish(...)` calls
  - `contractIds` — list of contract addresses to match
  - `topics` — ordered list of topic matchers; use `null` to wildcard a position
- `limit` — maximum events returned per call (default 20, max 10 000)
- `cursor` — opaque pagination cursor returned by the previous response

> **Ledger range limit**: `getEvents` can span at most ~17,280 ledgers (~24 h)
> per request. For longer ranges, paginate using the `cursor` field.

### Basic Snippet — Fetch Recent Events

```ts
import { SorobanRpc, Networks, xdr, scValToNative } from '@stellar/stellar-sdk';

const RPC_URL = 'https://soroban-testnet.stellar.org';
const server = new SorobanRpc.Server(RPC_URL, { allowHttp: false });

const TOKEN_CONTRACT_ID  = process.env.TOKEN_CONTRACT_ID!;
const ESCROW_CONTRACT_ID = process.env.ESCROW_CONTRACT_ID!;

/**
 * Fetch recent contract events from both the token and escrow contracts.
 * Returns raw event records from the RPC.
 */
async function fetchRecentEvents(startLedger: number) {
  const response = await server.getEvents({
    startLedger,
    filters: [
      {
        type: 'contract',
        contractIds: [TOKEN_CONTRACT_ID, ESCROW_CONTRACT_ID],
      },
    ],
    limit: 100,
  });

  return response.events;
}
```

### Filter by Contract ID and Event Topic

Use the `topics` array inside a filter to narrow results. Each element is either
a hex-encoded XDR `ScVal` or `null` (wildcard). The helper below builds topic
matchers for the common `Symbol` topic format used by these contracts.

```ts
import { xdr, ScVal, nativeToScVal } from '@stellar/stellar-sdk';

/** Encode a contract event name as a topic-filter string. */
function topicSymbol(name: string): string {
  // Topics in getEvents filters are hex-encoded XDR ScVal strings.
  return nativeToScVal(name, { type: 'symbol' }).toXDR('hex');
}

/** Encode an address as a topic-filter string. */
function topicAddress(address: string): string {
  return xdr.ScVal.scvAddress(
    xdr.ScAddress.scAddressTypeAccount(
      xdr.PublicKey.publicKeyTypeEd25519(
        Buffer.from(address, 'base64'), // adjust encoding for your SDK version
      ),
    ),
  ).toXDR('hex');
}

/**
 * Fetch all `transfer` events where `from` == senderAddress.
 * Topic layout: [Symbol("transfer"), Address(from), Address(to)]
 */
async function fetchTokenTransfers(startLedger: number, senderAddress: string) {
  const response = await server.getEvents({
    startLedger,
    filters: [
      {
        type: 'contract',
        contractIds: [TOKEN_CONTRACT_ID],
        topics: [
          [topicSymbol('transfer'), null, null], // match any transfer event
        ],
      },
    ],
    limit: 200,
  });

  // Decode topics and data from XDR
  return response.events.map((event) => {
    const topics = event.topic.map((t) => scValToNative(xdr.ScVal.fromXDR(t, 'hex')));
    const data   = scValToNative(xdr.ScVal.fromXDR(event.value, 'hex'));
    return { topics, data, ledger: event.ledger, id: event.id };
  });
}

/**
 * Fetch all `escrow_created` events (any buyer).
 * Topic layout: [Symbol("escrow_created"), Address(buyer), Address(seller)]
 */
async function fetchEscrowCreatedEvents(startLedger: number) {
  const response = await server.getEvents({
    startLedger,
    filters: [
      {
        type: 'contract',
        contractIds: [ESCROW_CONTRACT_ID],
        topics: [
          [topicSymbol('escrow_created'), null, null],
        ],
      },
    ],
    limit: 200,
  });

  return response.events.map((event) => {
    const topics = event.topic.map((t) => scValToNative(xdr.ScVal.fromXDR(t, 'hex')));
    const data   = scValToNative(xdr.ScVal.fromXDR(event.value, 'hex'));
    return {
      buyer:  topics[1] as string,
      seller: topics[2] as string,
      data,
      ledger: event.ledger,
    };
  });
}
```

### Polling for New Events

`getEvents` is a point-in-time query, not a push subscription. Poll on an
interval and advance `cursor` or `startLedger` each round:

```ts
/**
 * Poll the RPC every `intervalMs` milliseconds for new events.
 * Calls `onEvent` for each new event in order.
 */
async function pollEvents(
  startLedger: number,
  onEvent: (event: SorobanRpc.Api.RawEventResponse) => void,
  intervalMs = 5_000,
): Promise<void> {
  let cursor: string | undefined;

  while (true) {
    const params: SorobanRpc.Server.GetEventsRequest = {
      startLedger: cursor ? undefined : startLedger,
      filters: [
        {
          type: 'contract',
          contractIds: [TOKEN_CONTRACT_ID, ESCROW_CONTRACT_ID],
        },
      ],
      limit: 200,
      ...(cursor ? { cursor } : {}),
    };

    const response = await server.getEvents(params);

    for (const event of response.events) {
      onEvent(event);
      cursor = event.id; // advance cursor past this event
    }

    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

// Usage
const latestLedger = await server.getLatestLedger();
pollEvents(latestLedger.sequence, (event) => {
  const topics = event.topic.map((t) => scValToNative(xdr.ScVal.fromXDR(t, 'hex')));
  console.log('New event:', topics[0], 'ledger:', event.ledger);
});
```

### Decode a Specific Event Payload

Each contract's topic and data layout is documented in section 8 above. Here is
a complete decode example for the token `mint` event:

```ts
/**
 * Decode a raw token `mint` event into a typed record.
 * Topic layout: [Symbol("mint"), Address(admin), Address(to)]
 * Data:         i128 amount
 */
function decodeMintEvent(event: SorobanRpc.Api.RawEventResponse) {
  const [name, admin, to] = event.topic.map(
    (t) => scValToNative(xdr.ScVal.fromXDR(t, 'hex')),
  );
  const amount = scValToNative(xdr.ScVal.fromXDR(event.value, 'hex')) as bigint;
  return { name, admin, to, amount, ledger: event.ledger };
}
```

### CLI — Quick Event Inspection

The Stellar CLI `contract events` command is useful for debugging without
writing code:

```bash
# Fetch the last 10 events from the token contract
stellar contract events \
  --id "$TOKEN_CONTRACT_ID" \
  --network testnet \
  --count 10

# Filter to transfer events only
stellar contract events \
  --id "$TOKEN_CONTRACT_ID" \
  --network testnet \
  --filter-topics transfer
```

---

## 9. React / Frontend Integration

```tsx
import { WalletContext } from './context/WalletContext';
import { ContractInteraction } from './components/ContractInteraction';

function App() {
  return (
    <WalletContext>
      <ContractInteraction contractId="<CONTRACT_ID>" />
    </WalletContext>
  );
}
```

See `src/components/ContractInteractionUI.tsx` for a full working example.

---

## 10. Validation Checklist

- [ ] Contract ID is correct for the target network
- [ ] Network passphrase matches the RPC endpoint
- [ ] Source account has sufficient XLM for fees
- [ ] `wasm32-unknown-unknown` Rust target is installed
- [ ] Freighter is set to the matching network
- [ ] `.env` variables are set (see [dev-environment.md](dev-environment.md))

---

## 11. Troubleshooting

| Problem | Solution |
|---------|----------|
| `Transaction simulation failed` | Check contract ID and network passphrase match |
| `Account not found` | Fund the account via [Stellar Friendbot](https://friendbot.stellar.org) (testnet only) |
| `Insufficient fee` | Increase `BASE_FEE` or use `server.getFeeStats()` |
| `Contract not found` | Re-deploy with `./scripts/deploy.sh testnet` |
| Freighter not responding | Refresh page; ensure extension is unlocked |

---

## Security Considerations

- Never expose secret keys in frontend code — use Freighter or Albedo for signing
- Validate all user inputs before constructing `xdr.ScVal` arguments
- Use `server.simulateTransaction()` before submitting to catch errors cheaply
- Restrict admin operations to known addresses in contract initialization
- Rotate admin keys using the two-step `propose_admin` / `accept_admin` flow

For a full security guide covering key management, replay attack prevention,
front-running, upgrade timelocks, and event monitoring, see
[docs/security.md](security.md).

---

## Performance Tips

- Cache `getAccount()` results; re-fetch only on sequence number errors
- Batch read-only calls using `simulateTransaction` (no fee)
- Use `SorobanRpc.Server` with connection pooling for high-throughput apps
- Store contract IDs in environment variables, not hardcoded strings

---

## 12. Generating TypeScript Bindings

The Stellar CLI can generate typed TypeScript bindings directly from a deployed
contract. Use the provided script to automate this for every contract in
`.contract-ids`:

```bash
./scripts/generate-types.sh
```

Generated bindings land in `sdk/<contract-name>/` and are excluded from version
control via `.gitignore`. Regenerate them whenever a contract is upgraded.

### Override network or IDs file

```bash
STELLAR_NETWORK=mainnet ./scripts/generate-types.sh .contract-ids.mainnet
```

### Import the generated bindings

```ts
import * as TokenClient  from './sdk/token';
import * as EscrowClient from './sdk/escrow';

const token = new TokenClient.Client({ rpcUrl: '...', networkPassphrase: '...', contractId: '...' });
await token.balance({ id: buyerAddress });
```

---

## 13. TypeScript SDK Integration Examples

> **SDK Version**: These examples target `@stellar/stellar-sdk` **v13.x** (2026 stable), the current standard for Soroban smart contract integration. Install with:
> ```bash
> npm install @stellar/stellar-sdk@^13.0.0
> ```

The examples below are production-ready patterns with full error handling. They expand on the minimal snippets in Sections 6 and 7.

---

### Shared Setup

```ts
import {
  SorobanRpc,
  Networks,
  Keypair,
  TransactionBuilder,
  BASE_FEE,
  Contract,
  nativeToScVal,
  Address,
  xdr,
} from '@stellar/stellar-sdk';

const RPC_URL = 'https://soroban-testnet.stellar.org';
const NETWORK_PASSPHRASE = Networks.TESTNET;

const server = new SorobanRpc.Server(RPC_URL, { allowHttp: false });

/** Build, simulate, sign, and submit a transaction. */
async function invokeContract(
  sourceKeypair: Keypair,
  contractId: string,
  method: string,
  args: xdr.ScVal[],
): Promise<SorobanRpc.Api.GetSuccessfulTransactionResponse> {
  const account = await server.getAccount(sourceKeypair.publicKey());
  const contract = new Contract(contractId);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  // Simulate first to catch errors cheaply
  const simResult = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Simulation failed: ${simResult.error}`);
  }

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(sourceKeypair);

  const sendResult = await server.sendTransaction(prepared);
  if (sendResult.status === 'ERROR') {
    throw new Error(`Submission failed: ${JSON.stringify(sendResult.errorResult)}`);
  }

  // Poll for finality
  let getResult = await server.getTransaction(sendResult.hash);
  while (getResult.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND) {
    await new Promise((r) => setTimeout(r, 1000));
    getResult = await server.getTransaction(sendResult.hash);
  }

  if (getResult.status !== SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`Transaction failed: ${getResult.status}`);
  }

  return getResult as SorobanRpc.Api.GetSuccessfulTransactionResponse;
}
```

---

### Workflow A: Token Operations

#### A-1. Initialize Token Contract

```ts
/**
 * Initialize the token contract with admin, name, symbol, and decimals.
 * Must be called once after deployment; throws AlreadyInitialized on repeat calls.
 */
async function initializeToken(
  adminKeypair: Keypair,
  tokenContractId: string,
  name: string,
  symbol: string,
  decimals: number,
): Promise<void> {
  try {
    await invokeContract(adminKeypair, tokenContractId, 'initialize', [
      new Address(adminKeypair.publicKey()).toScVal(), // admin
      nativeToScVal(name, { type: 'string' }),         // name
      nativeToScVal(symbol, { type: 'string' }),       // symbol
      nativeToScVal(decimals, { type: 'u32' }),        // decimals
    ]);
    console.log(`Token "${symbol}" initialized on contract ${tokenContractId}`);
  } catch (err) {
    // AlreadyInitialized (error code 4) means the contract is already set up
    if (err instanceof Error && err.message.includes('AlreadyInitialized')) {
      console.warn('Token already initialized — skipping.');
    } else {
      console.error('initializeToken failed:', err);
      throw err;
    }
  }
}
```

#### A-2. Mint Tokens

```ts
/**
 * Mint tokens to a recipient. Requires admin keypair.
 * Amount is in the token's smallest unit (e.g., stroops for 7-decimal tokens).
 */
async function mintTokens(
  adminKeypair: Keypair,
  tokenContractId: string,
  recipientAddress: string,
  amount: bigint,
): Promise<void> {
  try {
    await invokeContract(adminKeypair, tokenContractId, 'mint', [
      new Address(recipientAddress).toScVal(),  // to
      nativeToScVal(amount, { type: 'i128' }),  // amount
    ]);
    console.log(`Minted ${amount} tokens to ${recipientAddress}`);
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('Unauthorized')) {
        console.error('Mint failed: caller is not the admin.');
      } else if (err.message.includes('InvalidAmount')) {
        console.error('Mint failed: amount must be positive and within max supply.');
      } else {
        console.error('mintTokens failed:', err.message);
      }
    }
    throw err;
  }
}
```

#### A-3. Transfer Tokens

```ts
/**
 * Transfer tokens between two accounts.
 * The source keypair must own the funds; no admin privileges required.
 */
async function transferTokens(
  senderKeypair: Keypair,
  tokenContractId: string,
  recipientAddress: string,
  amount: bigint,
): Promise<void> {
  try {
    await invokeContract(senderKeypair, tokenContractId, 'transfer', [
      new Address(senderKeypair.publicKey()).toScVal(), // from
      new Address(recipientAddress).toScVal(),          // to
      nativeToScVal(amount, { type: 'i128' }),          // amount
    ]);
    console.log(`Transferred ${amount} tokens to ${recipientAddress}`);
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('InsufficientBalance')) {
        console.error('Transfer failed: sender balance too low.');
      } else {
        console.error('transferTokens failed:', err.message);
      }
    }
    throw err;
  }
}
```

#### A-4. Approve Allowance

```ts
/**
 * Approve a spender to transfer up to `amount` tokens on the owner's behalf.
 * `expirationLedger` sets when the allowance expires (use 0 for no expiry).
 */
async function approveAllowance(
  ownerKeypair: Keypair,
  tokenContractId: string,
  spenderAddress: string,
  amount: bigint,
  expirationLedger: number,
): Promise<void> {
  try {
    await invokeContract(ownerKeypair, tokenContractId, 'approve', [
      new Address(ownerKeypair.publicKey()).toScVal(), // from (owner)
      new Address(spenderAddress).toScVal(),           // spender
      nativeToScVal(amount, { type: 'i128' }),         // amount
      nativeToScVal(expirationLedger, { type: 'u32' }),// expiration_ledger
    ]);
    console.log(`Approved ${amount} tokens for spender ${spenderAddress}`);
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('InvalidAmount')) {
        console.error('Approve failed: amount must be non-negative.');
      } else {
        console.error('approveAllowance failed:', err.message);
      }
    }
    throw err;
  }
}
```

---

### Workflow B: Escrow Operations

#### B-1. Initialize Escrow Contract

```ts
/**
 * Initialize the escrow with all parties and terms.
 * `deadline` is a Unix timestamp (seconds). `arbiter` may be the zero address
 * if no dispute resolution is needed.
 */
async function initializeEscrow(
  deployerKeypair: Keypair,
  escrowContractId: string,
  buyerAddress: string,
  sellerAddress: string,
  arbiterAddress: string,
  tokenContractId: string,
  amount: bigint,
  deadline: number,
): Promise<void> {
  try {
    await invokeContract(deployerKeypair, escrowContractId, 'initialize', [
      new Address(buyerAddress).toScVal(),              // buyer
      new Address(sellerAddress).toScVal(),             // seller
      new Address(arbiterAddress).toScVal(),            // arbiter
      new Address(tokenContractId).toScVal(),           // token
      nativeToScVal(amount, { type: 'i128' }),          // amount
      nativeToScVal(deadline, { type: 'u64' }),         // deadline
    ]);
    console.log(`Escrow initialized: ${escrowContractId}`);
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('AlreadyInitialized')) {
        console.warn('Escrow already initialized — skipping.');
      } else if (err.message.includes('InvalidParties')) {
        console.error('Escrow init failed: buyer, seller, and arbiter must be distinct.');
      } else {
        console.error('initializeEscrow failed:', err.message);
      }
    }
    throw err;
  }
}
```

#### B-2. Fund the Escrow

```ts
/**
 * Buyer funds the escrow by transferring the agreed amount into the contract.
 * The buyer must have previously approved the escrow contract as a spender
 * (see approveAllowance above) or hold sufficient balance for a direct transfer.
 */
async function fundEscrow(
  buyerKeypair: Keypair,
  escrowContractId: string,
): Promise<void> {
  try {
    await invokeContract(buyerKeypair, escrowContractId, 'fund', [
      new Address(buyerKeypair.publicKey()).toScVal(), // buyer (auth check)
    ]);
    console.log('Escrow funded successfully.');
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('InsufficientFunds')) {
        console.error('Fund failed: buyer token balance is too low.');
      } else if (err.message.includes('InvalidState')) {
        console.error('Fund failed: escrow is not in the expected state.');
      } else if (err.message.includes('DeadlinePassed')) {
        console.error('Fund failed: escrow deadline has already passed.');
      } else {
        console.error('fundEscrow failed:', err.message);
      }
    }
    throw err;
  }
}
```

#### B-3. Mark Delivery

```ts
/**
 * Seller marks the goods/service as delivered.
 * Transitions escrow state from Funded → Delivered.
 */
async function markDelivered(
  sellerKeypair: Keypair,
  escrowContractId: string,
): Promise<void> {
  try {
    await invokeContract(sellerKeypair, escrowContractId, 'mark_delivered', [
      new Address(sellerKeypair.publicKey()).toScVal(), // seller (auth check)
    ]);
    console.log('Delivery marked by seller.');
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('NotAuthorized')) {
        console.error('Mark delivered failed: caller is not the seller.');
      } else if (err.message.includes('InvalidState')) {
        console.error('Mark delivered failed: escrow must be in Funded state.');
      } else {
        console.error('markDelivered failed:', err.message);
      }
    }
    throw err;
  }
}
```

#### B-4. Approve Delivery and Release Funds

```ts
/**
 * Buyer (or arbiter) approves delivery, releasing escrowed funds to the seller.
 * Transitions escrow state from Delivered → Released.
 */
async function approveDelivery(
  buyerKeypair: Keypair,
  escrowContractId: string,
): Promise<void> {
  try {
    await invokeContract(buyerKeypair, escrowContractId, 'approve_delivery', [
      new Address(buyerKeypair.publicKey()).toScVal(), // buyer (auth check)
    ]);
    console.log('Delivery approved — funds released to seller.');
  } catch (err) {
    if (err instanceof Error) {
      if (err.message.includes('NotAuthorized')) {
        console.error('Approve delivery failed: caller is not the buyer or arbiter.');
      } else if (err.message.includes('InvalidState')) {
        console.error('Approve delivery failed: delivery must be marked first.');
      } else {
        console.error('approveDelivery failed:', err.message);
      }
    }
    throw err;
  }
}
```

---

### End-to-End Usage Example

```ts
// Configure keypairs and contract IDs from environment variables
const adminKeypair  = Keypair.fromSecret(process.env.ADMIN_SECRET!);
const buyerKeypair  = Keypair.fromSecret(process.env.BUYER_SECRET!);
const sellerKeypair = Keypair.fromSecret(process.env.SELLER_SECRET!);

const TOKEN_CONTRACT_ID  = process.env.TOKEN_CONTRACT_ID!;
const ESCROW_CONTRACT_ID = process.env.ESCROW_CONTRACT_ID!;

const AMOUNT = BigInt(100_000_000); // 10 tokens at 7 decimals

(async () => {
  // --- Token workflow ---
  await initializeToken(adminKeypair, TOKEN_CONTRACT_ID, 'MyToken', 'MTK', 7);
  await mintTokens(adminKeypair, TOKEN_CONTRACT_ID, buyerKeypair.publicKey(), AMOUNT);
  await approveAllowance(buyerKeypair, TOKEN_CONTRACT_ID, ESCROW_CONTRACT_ID, AMOUNT, 0);

  // --- Escrow workflow ---
  const deadline = Math.floor(Date.now() / 1000) + 7 * 24 * 60 * 60; // 7 days
  await initializeEscrow(
    adminKeypair, ESCROW_CONTRACT_ID,
    buyerKeypair.publicKey(), sellerKeypair.publicKey(),
    adminKeypair.publicKey(), // arbiter = admin for this example
    TOKEN_CONTRACT_ID, AMOUNT, deadline,
  );
  await fundEscrow(buyerKeypair, ESCROW_CONTRACT_ID);
  await markDelivered(sellerKeypair, ESCROW_CONTRACT_ID);
  await approveDelivery(buyerKeypair, ESCROW_CONTRACT_ID);

  console.log('Full token + escrow workflow completed successfully.');
})();
```

---

## 14. Resources

- [Stellar SDK Docs](https://stellar.github.io/js-stellar-sdk/)
- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Stellar Laboratory](https://laboratory.stellar.org/)
- [Freighter Wallet](https://freighter.app/)
