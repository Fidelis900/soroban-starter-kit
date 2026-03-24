#!/bin/bash

# Soroban Token Contract Deployment Script
# Usage: ./deploy.sh [network]
# Example: ./deploy.sh testnet

set -e

NETWORK=${1:-testnet}
CONTRACT_NAME="soroban-token-template"

echo "🚀 Deploying Token Contract to $NETWORK..."

# Build the contract
echo "📦 Building contract..."
soroban contract build

# Deploy the contract
echo "🌐 Deploying to $NETWORK..."
CONTRACT_ID=$(soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/${CONTRACT_NAME}.wasm \
    --network $NETWORK)

echo "✅ Token contract deployed!"
echo "📋 Contract ID: $CONTRACT_ID"

# Initialize the contract (example)
echo "🔧 Initializing contract..."
ADMIN_ADDRESS="GDXY2OEZQHIFKHDN7SWZQYN3JGMVGXD3UYEQMY4FIBWMHQPD5NEKZFIN"

soroban contract invoke \
    --id $CONTRACT_ID \
    --network $NETWORK \
    -- initialize \
    --admin $ADMIN_ADDRESS \
    --name "Example Token" \
    --symbol "EXT" \
    --decimals 18

echo "🎉 Token contract ready for use!"
echo "📝 Save this Contract ID: $CONTRACT_ID"