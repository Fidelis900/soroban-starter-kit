# Documentation Index

Welcome to the Soroban starter kit documentation. This directory contains guides and references for understanding and using the contract templates.

## Core Documentation

- **[System Architecture](architecture.md)** — High-level design overview
  - Contract relationship diagram
  - Storage tier choices and rationale
  - Event model and indexing guidance
  - Admin model and authorization
  - Feature flag matrix
  - Escrow state machine
  - Error handling reference

- **[Security Best Practices](security.md)** — Security analysis and guidelines
  - Re-entrancy prevention
  - Authorization checks
  - State machine invariants
  - Overflow/underflow protection

- **[Threat Model](threat-model.md)** — Trust assumptions and attack surface per contract
  - Trusted roles and what each can/cannot do
  - Worst-case-per-role compromise analysis

- **[Known Issues](known-issues.md)** — Accepted design tradeoffs per contract
  - Documented limitations that are not bugs
  - Cross-linked to relevant ADRs

- **[Glossary](glossary.md)** — Soroban/Stellar terminology used across the repo

- **[Integration Guide](integration-guide.md)** — How to integrate contracts into your application
  - Contract deployment
  - Client setup
  - Function calls and event handling
  - Error handling patterns

- **[Deployment Guide](deployment-guide.md)** — Step-by-step deployment instructions
  - Local development setup
  - Testnet deployment
  - Mainnet deployment
  - Monitoring and maintenance

## Development Resources

- **[Development Environment](dev-environment.md)** — Setting up your local development environment
  - Prerequisites
  - Installation
  - Local Stellar node setup
  - Testing and debugging

- **[Gas Costs](gas-costs.md)** — Gas cost analysis and optimization
  - Operation costs
  - Storage costs
  - Optimization strategies

## Proposals

- **[Proposals](proposals/README.md)**: Design notes for open issues (#1368–#1371: benchmarks, bench regression CI, cross-contract integration tests)

## Architecture Decision Records (ADRs)

The [adr/](adr/) directory contains detailed decision records for key design choices:

- **[ADR-0001: Storage Tier Choices](adr/0001-storage-tier-choices.md)** — Why we chose instance vs. persistent storage
- **[ADR-0003: Admin Model](adr/0003-admin-model.md)** — Admin authorization design
- **[ADR-0004: Escrow State Machine Design](adr/0004-escrow-state-machine.md)** — Escrow lifecycle and state transitions

## Quick Links

- [Token Contract Features](../README.md#token-contract-features)
- [Escrow Contract Features](../README.md#escrow-contract-features)
- [Error Reference](../README.md#-error-reference)
- [Contributing Guidelines](../CONTRIBUTING.md)
