#!/bin/bash

# Soroban Escrow Contract Deployment Script
# Usage: ./deploy.sh [network]
# Example: ./deploy.sh testnet

set -e

NETWORK=${1:-testnet}
CONTRACT_NAME="soroban-escrow-template"

echo "🚀 Deploying Escrow Contract to $NETWORK..."

# Build the contract
echo "📦 Building contract..."
soroban contract build

# Deploy the contract
echo "🌐 Deploying to $NETWORK..."
CONTRACT_ID=$(soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/${CONTRACT_NAME}.wasm \
    --network $NETWORK)

echo "✅ Escrow contract deployed!"
echo "📋 Contract ID: $CONTRACT_ID"

# Example initialization (uncomment to use)
# echo "🔧 Initializing escrow..."
# BUYER_ADDRESS="GDXY2OEZQHIFKHDN7SWZQYN3JGMVGXD3UYEQMY4FIBWMHQPD5NEKZFIN"
# SELLER_ADDRESS="GCKFBEIYTKP5RDBQMTVVALONAOPBXICILMAFOOBN244UFKB3LCFWKS7L"
# ARBITER_ADDRESS="GCKFBEIYTKP5RDBQMTVVALONAOPBXICILMAFOOBN244UFKB3LCFWKS7L"
# TOKEN_CONTRACT="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"
# AMOUNT=1000000000000000000000  # 1000 tokens with 18 decimals
# DEADLINE=$(($(date +%s) + 86400))  # 24 hours from now

# soroban contract invoke \
#     --id $CONTRACT_ID \
#     --network $NETWORK \
#     -- initialize \
#     --buyer $BUYER_ADDRESS \
#     --seller $SELLER_ADDRESS \
#     --arbiter $ARBITER_ADDRESS \
#     --token_contract $TOKEN_CONTRACT \
#     --amount $AMOUNT \
#     --deadline_ledger $DEADLINE

echo "🎉 Escrow contract ready for use!"
echo "📝 Save this Contract ID: $CONTRACT_ID"