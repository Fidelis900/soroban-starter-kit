# Soroban Contract Templates

A curated, well-documented collection of production-ready, modular Soroban smart contract templates and examples. These help new and experienced developers quickly bootstrap common use cases on Soroban (Stellar's smart contract platform), lowering the barrier to building dApps, DeFi, NFTs, payments, and more.

This repository aims to fill gaps beyond the official [stellar/soroban-examples](https://github.com/stellar/soroban-examples) by including more real-world patterns, best practices, comprehensive tests, deployment scripts, and frontend integration stubs.

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/your-username/soroban-contract-templates.git
cd soroban-contract-templates

# Build a contract (example: token)
cd contracts/token
soroban contract build

# Deploy to testnet
./scripts/deploy.sh testnet

# Run tests
cargo test
```

## 📦 Included Templates

| Template | Description | Use Cases | Status |
|----------|-------------|-----------|---------|
| **Token** | Custom fungible token with mint/burn/admin controls | DeFi tokens, governance tokens, utility tokens | ✅ Complete |
| **Escrow** | Two-party escrow with timeout and refund mechanism | P2P trading, service payments, milestone payments | ✅ Complete |
| **Voting** | Governance/DAO voting with balance-weighted votes | DAOs, community governance, proposal systems | 🚧 Planned |
| **Subscription** | Recurring payment streaming using timestamps | SaaS payments, content subscriptions, memberships | 🚧 Planned |
| **NFT Mint** | Simple NFT minting with metadata storage | Digital collectibles, certificates, gaming assets | 🚧 Planned |

### Token Contract Features
- **Standard Interface**: Full Soroban token compatibility
- **Administrative Controls**: Mint, burn, and admin management
- **Metadata Support**: Name, symbol, and decimals
- **Allowance System**: Approve and transfer_from functionality
- **Event Emission**: All operations emit events for tracking
- **Error Handling**: Custom error types for better debugging

### Escrow Contract Features
- **Two-Party Security**: Secure buyer-seller transactions
- **Deadline Protection**: Automatic refunds after deadline
- **Arbiter Support**: Third-party dispute resolution
- **State Management**: Clear transaction lifecycle
- **Token Agnostic**: Works with any Soroban token
- **Event Emission**: All operations emit events for tracking

Each template includes:
- ✅ Complete contract implementation
- ✅ Comprehensive unit tests (8+ test cases each)
- ✅ Deployment scripts with examples
- ✅ Usage examples and documentation
- ✅ React Native mobile integration

## 🛠 Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup#install-the-soroban-cli)
- [Node.js](https://nodejs.org/) (for React Native mobile app)
- [React Native development environment](https://reactnative.dev/docs/environment-setup)

## 📖 Usage

### Building Contracts

```bash
cd contracts/[template-name]
soroban contract build
```

### Running Tests

```bash
cd contracts/[template-name]
cargo test
```

### Deploying to Testnet

```bash
cd contracts/[template-name]
./scripts/deploy.sh testnet
```

### Frontend Integration

Each contract includes a complete React Native mobile app example in `examples/react-native/` showing how to:
- Connect with Stellar wallets (Freighter, Albedo)
- Deploy contracts from mobile
- Interact with all contract functions
- Handle transaction states and errors
- Provide native iOS/Android user experience

```bash
# Run the React Native example
cd examples/react-native
npm install
npm run ios  # or npm run android
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on:
- Adding new contract templates
- Improving existing contracts
- Fixing bugs and issues
- Writing documentation

## 🌊 Stellar Wave Program

This repository is applying to the [Stellar Wave Program](https://www.drips.network/wave) on Drips.network! Look for issues labeled `stellar-wave` for bounty opportunities to contribute and earn rewards.

### How to Contribute for Rewards:
1. Browse [open issues](https://github.com/your-username/soroban-contract-templates/issues)
2. Look for `stellar-wave` labeled issues with point values
3. Comment to claim an issue
4. Submit a PR with your solution
5. Earn rewards through Drips after PR approval!

## 📚 Resources

- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Stellar Developer Discord](https://discord.gg/stellardev)
- [Soroban Examples](https://github.com/stellar/soroban-examples)
- [Freighter Wallet](https://freighter.app/)
- [Stellar Laboratory](https://laboratory.stellar.org/)

## 📄 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Stellar Development Foundation for Soroban
- The Stellar developer community
- Contributors and maintainers

---

**Ready to build on Soroban?** Start with any template and customize it for your use case! 🚀