#!/usr/bin/env node
/**
 * Minimal end-to-end example: deploy token, mint to buyer, run full escrow lifecycle.
 * Targets a local Stellar node (http://localhost:8000).
 *
 * Prerequisites:
 *   npm install @stellar/stellar-sdk
 *   ./scripts/local-net.sh start
 *   ./scripts/deploy.sh local
 *
 * Usage:
 *   TOKEN_CONTRACT_ID=<id> ESCROW_CONTRACT_ID=<id> node examples/typescript/index.js
 */

const {
  Keypair,
  SorobanRpc,
  TransactionBuilder,
  Networks,
  BASE_FEE,
  Contract,
  xdr,
  nativeToScVal,
  Address,
} = require('@stellar/stellar-sdk');

const RPC_URL = process.env.SOROBAN_RPC_URL || 'http://localhost:8000/soroban/rpc';
const NETWORK_PASSPHRASE = process.env.NETWORK_PASSPHRASE || 'Standalone Network ; February 2017';
const TOKEN_CONTRACT_ID = process.env.TOKEN_CONTRACT_ID;
const ESCROW_CONTRACT_ID = process.env.ESCROW_CONTRACT_ID;

if (!TOKEN_CONTRACT_ID || !ESCROW_CONTRACT_ID) {
  console.error('Set TOKEN_CONTRACT_ID and ESCROW_CONTRACT_ID environment variables.');
  process.exit(1);
}

const server = new SorobanRpc.Server(RPC_URL, { allowHttp: true });

async function invokeContract(sourceKeypair, contractId, method, args) {
  const account = await server.getAccount(sourceKeypair.publicKey());
  const contract = new Contract(contractId);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(sourceKeypair);

  const result = await server.sendTransaction(prepared);
  if (result.status === 'ERROR') throw new Error(`Transaction failed: ${JSON.stringify(result)}`);

  let response = result;
  while (response.status === 'PENDING' || response.status === 'NOT_FOUND') {
    await new Promise(r => setTimeout(r, 1000));
    response = await server.getTransaction(result.hash);
  }
  return response;
}

async function main() {
  const admin = Keypair.random();
  const buyer = Keypair.random();
  const seller = Keypair.random();
  const arbiter = Keypair.random();

  console.log('Admin:', admin.publicKey());
  console.log('Buyer:', buyer.publicKey());
  console.log('Seller:', seller.publicKey());
  console.log('Arbiter:', arbiter.publicKey());

  // Fund accounts via friendbot (local node)
  for (const kp of [admin, buyer, seller, arbiter]) {
    await fetch(`http://localhost:8000/friendbot?addr=${kp.publicKey()}`);
    console.log(`Funded ${kp.publicKey().slice(0, 8)}...`);
  }

  const MINT_AMOUNT = 1_000_000n;
  const ESCROW_AMOUNT = 500_000n;

  // The contract measures deadlines in ledger sequence numbers, not Unix
  // time. A local standalone network closes a ledger roughly every 5s, so
  // ~720 ledgers is roughly one hour out.
  const latestLedger = await server.getLatestLedger();
  const DEADLINE_LEDGER = latestLedger.sequence + 720;
  const DISPUTE_TIMEOUT_LEDGERS = 100;

  console.log('\n--- Minting tokens to buyer ---');
  await invokeContract(admin, TOKEN_CONTRACT_ID, 'mint', [
    new Address(buyer.publicKey()).toScVal(),
    nativeToScVal(MINT_AMOUNT, { type: 'i128' }),
  ]);
  console.log(`Minted ${MINT_AMOUNT} tokens to buyer`);

  console.log('\n--- Creating escrow ---');
  await invokeContract(buyer, ESCROW_CONTRACT_ID, 'initialize', [
    new Address(admin.publicKey()).toScVal(),
    new Address(buyer.publicKey()).toScVal(),
    new Address(seller.publicKey()).toScVal(),
    new Address(arbiter.publicKey()).toScVal(),
    new Address(TOKEN_CONTRACT_ID).toScVal(),
    nativeToScVal(ESCROW_AMOUNT, { type: 'i128' }),
    nativeToScVal(DEADLINE_LEDGER, { type: 'u32' }),
    nativeToScVal(DISPUTE_TIMEOUT_LEDGERS, { type: 'u32' }),
    xdr.ScVal.scvVoid(), // metadata_hash: None
  ]);
  console.log('Escrow created');

  console.log('\n--- Funding escrow ---');
  await invokeContract(buyer, ESCROW_CONTRACT_ID, 'fund', []);
  console.log('Escrow funded');

  console.log('\n--- Marking delivery ---');
  await invokeContract(seller, ESCROW_CONTRACT_ID, 'mark_delivered', []);
  console.log('Delivery marked');

  console.log('\n--- Approving delivery (releasing funds to seller) ---');
  await invokeContract(buyer, ESCROW_CONTRACT_ID, 'approve_delivery', []);
  console.log('Funds released to seller');

  console.log('\nFull escrow lifecycle complete.');
}

main().catch(err => { console.error(err); process.exit(1); });
