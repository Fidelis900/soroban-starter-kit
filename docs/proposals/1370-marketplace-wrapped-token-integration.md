# #1370: Integration Test for a Marketplace Purchase Paid in Wrapped Tokens

_Status: Proposed · Tracking issue: #1370_

## Goal

Add an end-to-end test to `tests/tests/integration.rs` that exercises three
contracts in one flow:

1. A seller mints an NFT on `NftContract` and lists it on `MarketplaceContract`.
2. A buyer wraps an underlying asset on `WrappedTokenContract`.
3. The buyer purchases the NFT, paying in the wrapped token.
4. Assertions: the buyer owns the NFT, the creator received royalties, and the
   seller received wrapped tokens.

## Blockers to resolve first

Two things in the current tree must be fixed before this test can pass:

1. **The integration crate does not compile.**
   `cargo test -p soroban-integration-tests --no-run` currently fails while
   building `soroban-lottery-template`. Also, the existing
   `test_marketplace_nft_token_end_to_end` calls `nft.mint(&seller, &token_id, ...)`,
   but `NftContract::mint` is now `mint(to, token_uri, royalty_bps, royalty_recipient) -> u32`.
   Fix these first, or the new test can't run.
2. **`NftContract` has no `transfer_from`.** `MarketplaceContract::buy` calls
   `NftClient::transfer_from(spender, from, to, token_id)` (declared through
   `#[contractclient]` in `contracts/marketplace/src/lib.rs`), but
   `contracts/nft/src/lib.rs` only exports `transfer` and `approve`. Only
   `nft/src/test.rs` refers to `transfer_from`. Until the NFT contract
   implements it, `buy` will trap when it tries to move the NFT.

## Contract wiring

| Role | Contract | Setup |
|---|---|---|
| Underlying asset | Stellar Asset Contract | `env.register_stellar_asset_contract_v2(issuer)`; mint to the buyer |
| Wrapped token | Stellar Asset Contract | Second SAC whose **admin is set to the wrapper contract**, because `wrap` calls `StellarAssetClient::mint` |
| Wrapper | `WrappedTokenContract` | `initialize(admin, wrapped_sac, underlying_sac, None)` |
| NFT | `NftContract` | `initialize(...)`; `mint(&seller, &uri, &Some(royalty_bps), &Some(creator))` returns `token_id` |
| Marketplace | `MarketplaceContract` | `initialize(admin, wrapped_sac, default_royalty_bps, default_recipient)`. The payment token is the **wrapped** SAC |

`MarketplaceContract::initialize` calls `decimals()` on the payment token, so
the wrapped SAC must exist before the marketplace is initialized.

Royalty source: `resolve_royalty` prefers the NFT's per-token
`royalty_info` and falls back to the marketplace default. Mint with a
per-token royalty and a distinct `creator` address so the assertion checks the
NFT path, not the default.

## Test outline

```rust
#[test]
fn test_marketplace_purchase_with_wrapped_tokens() {
    let env = Env::default();
    env.mock_all_auths();

    // actors: issuer, wrap_admin, nft_admin, mkt_admin, creator, seller, buyer
    // 1. underlying SAC → mint 10_000 to buyer
    // 2. wrapped SAC; register WrappedTokenContract; set_admin(wrapped SAC → wrapper)
    // 3. wrapper.initialize(...)
    // 4. nft.initialize(...); token_id = nft.mint(&seller, &uri, &Some(500), &Some(creator))
    // 5. marketplace.initialize(&mkt_admin, &wrapped_sac, &0, &mkt_admin)
    // 6. nft.approve(&seller, &marketplace_addr, &token_id); listing = marketplace.list(&seller, &nft_addr, &token_id, &price)
    // 7. wrapper.wrap(&buyer, &price)
    // 8. marketplace.buy(&buyer, &listing)
    // 9. assertions (below)
}
```

## Assertions

**Ownership and balances:**

| Check | Expected |
|---|---|
| `nft.owner_of(token_id)` | `buyer` |
| `wrapped.balance(buyer)` | `0` (wrapped exactly `price`) |
| `wrapped.balance(seller)` | `price - royalty` |
| `wrapped.balance(creator)` | `royalty = price * 500 / 10_000` |
| `underlying.balance(wrapper_addr)` | `price` (reserve backs the supply) |
| `wrapper.get_total_wrapped()` | `price` |
| `marketplace.get_listing(listing).active` | `false` |

Conservation: `seller + creator` wrapped balances equal `price`, and the
wrapped token's total supply equals the underlying reserve.

**Events.** Filter `env.events().all()` by contract address and check:

| Contract | Topic | Data |
|---|---|---|
| `WrappedTokenContract` | `("wrapped",)` | `(buyer, price, total)` |
| `MarketplaceContract` | `("listed", listing_id)` | seller, price |
| `MarketplaceContract` | `("sold", listing_id)` | `(buyer, price)` |
| `NftContract` | `("minted", seller)` | `token_id` |
| `NftContract` | `("transferred", seller, buyer)` | `token_id` (depends on how `transfer_from` is implemented) |

The wrapped SAC also emits `mint` and `transfer` events. Asserting the two
`transfer` events (buyer → seller, buyer → creator) confirms the royalty split
at the token level.

**Atomicity (second test).** Wrap less than `price`, call
`marketplace.try_buy(...)`, and assert that it returns an error, that the NFT
is still owned by the seller, that the listing is still active, and that no
wrapped balances changed. This shows the cross-contract call rolls back as a
unit.

## Running

```bash
cargo test -p soroban-integration-tests --test integration marketplace_purchase_with_wrapped
```

Add `soroban-wrapped-token-template = { path = "../contracts/wrapped-token" }`
(check the package name) to `tests/Cargo.toml`.

## Acceptance criteria

- [ ] The cross-contract test is in `tests/tests/integration.rs`
- [ ] Multi-contract atomic transfer and balance accounting are verified, including the failure/rollback case
- [ ] Events from the NFT, marketplace and wrapped-token contracts are asserted
