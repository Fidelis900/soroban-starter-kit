#!/usr/bin/env bash
# Minimal end-to-end example using the Stellar CLI:
# deploys token, mints to buyer, runs full escrow lifecycle against a local node.
#
# Prerequisites:
#   stellar-cli installed (cargo install --locked stellar-cli --features opt)
#   ./scripts/local-net.sh start
#
# Usage:
#   ./examples/shell/run.sh
set -euo pipefail

NETWORK="${STELLAR_NETWORK:-local}"
RPC_URL="${SOROBAN_RPC_URL:-http://localhost:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"

MINT_AMOUNT=1000000
ESCROW_AMOUNT=500000
DISPUTE_TIMEOUT_LEDGERS=100

echo "=== Generating keypairs ==="
stellar keys generate admin   --network "$NETWORK" --overwrite
stellar keys generate buyer   --network "$NETWORK" --overwrite
stellar keys generate seller  --network "$NETWORK" --overwrite
stellar keys generate arbiter --network "$NETWORK" --overwrite

ADMIN_KEY=$(stellar keys address admin)
BUYER_KEY=$(stellar keys address buyer)
SELLER_KEY=$(stellar keys address seller)
ARBITER_KEY=$(stellar keys address arbiter)

echo "Admin:   $ADMIN_KEY"
echo "Buyer:   $BUYER_KEY"
echo "Seller:  $SELLER_KEY"
echo "Arbiter: $ARBITER_KEY"

echo ""
echo "=== Funding accounts via friendbot ==="
curl -sf "http://localhost:8000/friendbot?addr=$ADMIN_KEY"   > /dev/null
curl -sf "http://localhost:8000/friendbot?addr=$BUYER_KEY"   > /dev/null
curl -sf "http://localhost:8000/friendbot?addr=$SELLER_KEY"  > /dev/null
curl -sf "http://localhost:8000/friendbot?addr=$ARBITER_KEY" > /dev/null
echo "Funded all accounts"

echo ""
echo "=== Building and deploying contracts ==="
stellar contract build --manifest-path contracts/token/Cargo.toml  2>/dev/null
stellar contract build --manifest-path contracts/escrow/Cargo.toml 2>/dev/null

TOKEN_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/token.wasm \
  --source admin \
  --network "$NETWORK")

ESCROW_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source admin \
  --network "$NETWORK")

echo "Token  contract: $TOKEN_ID"
echo "Escrow contract: $ESCROW_ID"

echo ""
echo "=== Initializing token contract ==="
stellar contract invoke --id "$TOKEN_ID" --source admin --network "$NETWORK" \
  -- initialize \
  --admin "$ADMIN_KEY" \
  --name "DemoToken" \
  --symbol "DEMO" \
  --decimals 7

echo ""
echo "=== Minting tokens to buyer ==="
stellar contract invoke --id "$TOKEN_ID" --source admin --network "$NETWORK" \
  -- mint \
  --to "$BUYER_KEY" \
  --amount "$MINT_AMOUNT"
echo "Minted $MINT_AMOUNT DEMO to buyer"

echo ""
echo "=== Computing escrow deadline ==="
# The contract measures deadlines in ledger sequence numbers, not Unix time.
# A local standalone network closes a ledger roughly every 5s, so ~720
# ledgers is roughly one hour out.
LATEST_LEDGER=$(curl -sf -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger","params":{}}' \
  | grep -o '"sequence":[0-9]*' | head -1 | cut -d: -f2)
DEADLINE_LEDGER=$(( LATEST_LEDGER + 720 ))
echo "Latest ledger: $LATEST_LEDGER, deadline ledger: $DEADLINE_LEDGER"

echo ""
echo "=== Creating escrow ==="
stellar contract invoke --id "$ESCROW_ID" --source buyer --network "$NETWORK" \
  -- initialize \
  --admin   "$ADMIN_KEY" \
  --buyer   "$BUYER_KEY" \
  --seller  "$SELLER_KEY" \
  --arbiter "$ARBITER_KEY" \
  --token_contract "$TOKEN_ID" \
  --amount "$ESCROW_AMOUNT" \
  --deadline_ledger "$DEADLINE_LEDGER" \
  --dispute_timeout_ledgers "$DISPUTE_TIMEOUT_LEDGERS"
echo "Escrow created"

echo ""
echo "=== Funding escrow ==="
stellar contract invoke --id "$ESCROW_ID" --source buyer --network "$NETWORK" \
  -- fund
echo "Escrow funded"

echo ""
echo "=== Marking delivery ==="
stellar contract invoke --id "$ESCROW_ID" --source seller --network "$NETWORK" \
  -- mark_delivered
echo "Delivery marked"

echo ""
echo "=== Approving delivery (releasing funds to seller) ==="
stellar contract invoke --id "$ESCROW_ID" --source buyer --network "$NETWORK" \
  -- approve_delivery
echo "Funds released"

echo ""
echo "Full escrow lifecycle complete."
