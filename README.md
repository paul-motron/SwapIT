# SwapIT — Trustless Atomic Swaps on Stellar

[![CI](https://github.com/paul-motron/SwapIT/actions/workflows/ci.yml/badge.svg)](https://github.com/paul-motron/SwapIT/actions/workflows/ci.yml)
[![Security Audit](https://github.com/paul-motron/SwapIT/actions/workflows/ci.yml/badge.svg)](https://github.com/paul-motron/SwapIT/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/paul-motron/SwapIT/branch/main/graph/badge.svg)](https://codecov.io/gh/paul-motron/SwapIT)

A trustless atomic swap protocol built on Stellar Soroban. Two parties swap an asset for payment in one transaction — the buyer pays into escrow, the seller reveals the secret that unlocks their side, and the contract enforces that either both sides settle or neither does.

## 🎯 What is SwapIT?

SwapIT is a hash-locked atomic swap contract for Stellar. Sellers register a swappable asset behind a hiding commitment, buyers pay into the contract, and funds only move when the seller reveals the secret that opens that commitment — the same mechanism behind cross-chain atomic swaps and Lightning-style HTLCs, applied to on-chain settlement:

- Register an asset under a commitment (`sha256(secret ‖ blinding_factor)`) without revealing either value
- Initiate a swap for a price, in any supported token
- Buyer accepts and funds are held in escrow
- Seller reveals the secret; the contract verifies it and releases payment atomically
- If the seller never reveals, the buyer recovers their funds after expiry

## 🚀 Features

The `atomic_swap` contract has grown well beyond a bare swap primitive:

- **Escrow & dispute resolution** — buyer/seller disputes, an M-of-N arbitrator committee, time-locked rulings, and non-refundable dispute bonds
- **Price oracles** — signed price attestations with staleness checks and deviation bounds, so swaps can settle at a live market price
- **Multi-currency support** — settle in any configured token, not just XLM
- **Batch operations** — initiate, accept, reveal, cancel, or approve many swaps in one call
- **Reputation** — seller/buyer reputation scores that can gate swap acceptance
- **Insurance pool** — optional buyer-side coverage funded by premiums, paid out if a swap fails
- **Auctions & installments** — auction an asset to the highest bidder, or pay for it in installments
- **Multi-signer reveal** — require several co-signers before a key reveal completes
- **Renegotiation, referrals, escrow agents** — price renegotiation mid-swap, referral fees on completion, optional third-party escrow agents for high-value trades
- **Upgrade safety** — an on-chain schema manifest that validates new contract versions stay backward compatible

## 🛠️ Quick Start

### Prerequisites

- Rust (1.70+)
- Soroban CLI
- Stellar CLI
- Node.js (16+) and npm (for JS layer testing)

### Build

```bash
./scripts/build.sh
```

### Test

Run all tests (Rust + JS):

```bash
# Run Rust tests
./scripts/test.sh

# Run JS tests
npm test

# Run JS tests with coverage
npm run test:coverage

# Watch mode for JS tests
npm run test:watch
```

The JS test suite covers batch operations (cancellation, dispute resolution, fee calculation, etc.) and SDK modules.

### Deploy to Testnet

```bash
# Configure your testnet identity first
stellar keys generate deployer --network testnet

# Deploy
./scripts/deploy_testnet.sh
```

## 🌐 Testnet Deployment Status

[![Deploy to Testnet](https://github.com/paul-motron/SwapIT/actions/workflows/deploy-testnet.yml/badge.svg)](https://github.com/paul-motron/SwapIT/actions/workflows/deploy-testnet.yml)

Latest testnet deployment addresses are published in GitHub Actions deployment summaries. Deployments are triggered automatically on release tags (`v*`).

## 📖 Documentation

### Core Documentation
- [Architecture Overview](docs/architecture.md)
- [Commitment Scheme](docs/commitment-scheme.md)
- [Atomic Swap Flow](docs/atomic-swap.md)
- [Threat Model & Security](docs/threat-model.md)
- [Integration Guide for Wallet Providers](docs/integration-guide.md)

### Additional Resources
- [API Reference](docs/api-reference.md)
- [Security Policy](SECURITY.md)

## 📦 Release Notes and Changelog

Release notes are generated automatically from commit messages and PR metadata. Push a tag in the format `v*` (e.g., `v1.2.0`) to trigger the release workflow.

## 🎓 Smart Contract API

### Asset Commitments

```rust
register_asset(owner, commitment_hash) -> u64     // Register a swappable asset under a hiding commitment
get_asset(asset_id) -> AssetCommitment            // Retrieve an asset's owner, commitment, and revoked flag
revoke_asset(owner, asset_id)                     // Revoke an asset so it can no longer be swapped
```

### Atomic Swap

```rust
initiate_swap(token, asset_id, seller, price, buyer, ...) -> u64  // Seller initiates a swap
accept_swap(swap_id)                              // Buyer accepts; payment moves into escrow
reveal_key(swap_id, seller, secret, blinding_factor)  // Seller reveals the secret; payment releases
cancel_swap(swap_id, caller)                      // Cancel a pending swap, or an expired accepted one

// Price Oracle Integration
set_oracle(caller, oracle_address, oracle_pubkey, enabled, max_deviation_bps)
get_oracle_config() -> Option<OracleConfig>       // Query current oracle configuration
get_oracle_price(token) -> i128                   // Fetch current price from oracle
initiate_swap_with_oracle_price(...) -> u64       // Initiate a swap priced from the oracle

// Batch operations
batch_initiate_swap(token, asset_ids, seller, prices, buyer, ...) -> Vec<u64>
batch_accept_swaps(swap_ids, buyer)
batch_reveal_keys(swap_ids, secrets, blinding_factors, seller)
```

See [docs/api-reference.md](docs/api-reference.md) for the full surface, including arbitration, auctions, installments, insurance, and reputation.

## 🧪 Testing

Comprehensive test suite covering:

✅ Asset registration and commitment hiding
✅ Atomic swap initiation and acceptance
✅ Key reveal and payment release
✅ Invalid key rejection and payment refund
✅ Dispute, arbitration, and rollback flows
✅ Error handling and edge cases

Run tests:

```bash
cargo test
```

## 🌍 Why This Matters

Escrow today usually means a trusted third party who can freeze funds, take a cut, or simply disappear. Atomic swaps remove that party: the same transaction that pays the seller is the one that proves they delivered, enforced by the contract instead of a middleman.

Benefits:

- No custodian holding funds mid-trade
- Payment and delivery are the same atomic event — no counterparty risk
- Disputes fall back to an arbitrator committee, not a single point of failure
- Accessible to anyone with a Stellar wallet

## 🗺️ Roadmap

- v1.0 (Current): Multi-currency swaps, escrow, arbitration, oracles, batch operations
- v1.1: Expanded settlement token support and fee tooling
- v2.0: Partial/conditional reveal flows for complex trade terms
- v3.0: Frontend UI with wallet integration
- v4.0: Mobile app

## 🤝 Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Stellar Development Foundation](https://stellar.org) for Soroban
- The global engineering community building without borders
- Drips Wave for supporting public goods funding
