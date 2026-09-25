#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::as_conversions
)]
#![no_main]

//! Stateful fuzz target for the marketplace order book and offer escrow (#1100).
//!
//! Each 6-byte chunk of input decodes to one operation (list, list_with_expiry,
//! buy, make_offer, accept_offer, cancel_offer, cancel, sweep_expired,
//! make/accept/cancel_collection_offer, advance ledger). After every operation
//! the target asserts:
//! - marketplace token balance == sum of all escrowed listing + collection offers;
//! - total payment-token supply is conserved;
//! - an NFT only changes owner through a successful sale that paid its previous owner.
//! At the end every outstanding offer is cancelled and the marketplace must hold zero.

use libfuzzer_sys::fuzz_target;
use soroban_marketplace_template::{
    MarketplaceContract, MarketplaceContractClient, RoyaltyInfo,
};
use soroban_sdk::{
    Address, Env, contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

#[contracttype]
enum NftKey {
    Owner(u32),
    Approved(u32),
}

#[contract]
pub struct FuzzNft;

#[contractimpl]
impl FuzzNft {
    pub fn init(env: Env, owner: Address, token_id: u32, spender: Address) {
        env.storage().persistent().set(&NftKey::Owner(token_id), &owner);
        env.storage()
            .persistent()
            .set(&NftKey::Approved(token_id), &spender);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32) {
        let approved: Address = env
            .storage()
            .persistent()
            .get(&NftKey::Approved(token_id))
            .expect("no approval");
        assert_eq!(approved, spender);
        let owner: Address = env
            .storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .expect("no owner");
        assert_eq!(owner, from);
        env.storage().persistent().set(&NftKey::Owner(token_id), &to);
    }

    pub fn owner_of(env: Env, token_id: u32) -> Address {
        env.storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .expect("token not found")
    }

    pub fn royalty_info(_env: Env, _token_id: u32, _sale_price: i128) -> Option<RoyaltyInfo> {
        None
    }
}

const TOKENS: u32 = 4;
const BUYERS: usize = 3;
const MINT: i128 = 10_000_000;

fuzz_target!(|data: &[u8]| {
    if data.len() < 6 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|l| l.sequence_number = 100);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let legacy_recipient = Address::generate(&env);
    let creator_a = Address::generate(&env);
    let creator_b = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let sac = StellarAssetClient::new(&env, &token_addr);
    let tok = TokenClient::new(&env, &token_addr);

    let marketplace = env.register_contract(None, MarketplaceContract);
    let client = MarketplaceContractClient::new(&env, &marketplace);
    client.initialize(&admin, &token_addr, &250u32, &legacy_recipient);
    // Exercise multi-recipient splits when the first byte is odd.
    if data[0] & 1 == 1 {
        client.set_royalty_splits(
            &admin,
            &soroban_sdk::vec![&env, (creator_a.clone(), 300u32), (creator_b.clone(), 150u32)],
        );
    }

    let nft_addr = env.register_contract(None, FuzzNft);
    let nft = FuzzNftClient::new(&env, &nft_addr);
    for id in 1..=TOKENS {
        nft.init(&seller, &id, &marketplace);
    }

    let buyers: Vec<Address> = (0..BUYERS).map(|_| Address::generate(&env)).collect();
    for b in &buyers {
        sac.mint(b, &MINT);
    }
    sac.mint(&seller, &MINT);
    let total_supply = MINT * (BUYERS as i128 + 1);

    let mut actors = buyers.clone();
    actors.extend([seller.clone(), legacy_recipient, creator_a, creator_b]);

    let mut listings: u64 = 0;
    let mut collection_offers: u64 = 0;

    let escrowed = |listings: u64, collection_offers: u64| -> i128 {
        let mut sum = 0i128;
        for l in 0..listings {
            for b in &buyers {
                sum += client.get_offer(&l, b).unwrap_or(0);
            }
        }
        for o in 0..collection_offers {
            if let Some(offer) = client.get_collection_offer(&o) {
                sum += offer.amount;
            }
        }
        sum
    };

    for chunk in data[1..].chunks_exact(6) {
        let (op, a, b, c) = (chunk[0], chunk[1], chunk[2], u16::from_le_bytes([chunk[3], chunk[4]]));
        let token = u32::from(a) % TOKENS + 1;
        let buyer = &buyers[usize::from(a) % BUYERS];
        let listing = u64::from(b) % (listings + 1);
        let amount = i128::from(c).max(1);
        let ttl = u32::from(chunk[5] % 32) + 1;
        let seq = env.ledger().sequence();

        let owners_before: Vec<Address> = (1..=TOKENS).map(|id| nft.owner_of(&id)).collect();
        let balances_before: Vec<i128> = actors.iter().map(|x| tok.balance(x)).collect();
        let mut sold = false;

        match op % 12 {
            0 => {
                let owner = nft.owner_of(&token);
                if client.try_list(&owner, &nft_addr, &token, &amount).is_ok() {
                    listings += 1;
                }
            }
            1 => {
                let owner = nft.owner_of(&token);
                if client
                    .try_list_with_expiry(&owner, &nft_addr, &token, &amount, &(seq + ttl))
                    .is_ok()
                {
                    listings += 1;
                }
            }
            2 => {
                if let Some(l) = client.get_listing(&listing) {
                    // chunk[5] bit 7 set => buyer's max price is below the listing price.
                    let max_price = if chunk[5] & 0x80 != 0 { l.price - 1 } else { l.price };
                    let res = client.try_buy(buyer, &listing, &max_price);
                    if max_price < l.price {
                        assert!(res.is_err(), "buy executed above buyer's max price");
                    }
                    sold = res.is_ok();
                }
            }
            3 => {
                let _ = client.try_make_offer(buyer, &listing, &amount);
            }
            4 => {
                if let Some(l) = client.get_listing(&listing) {
                    sold = client.try_accept_offer(&l.seller, &listing, buyer).is_ok();
                }
            }
            5 => {
                let _ = client.try_cancel_offer(buyer, &listing);
            }
            6 => {
                if let Some(l) = client.get_listing(&listing) {
                    let _ = client.try_cancel(&l.seller, &listing);
                }
            }
            7 => {
                if let Some(l) = client.get_listing(&listing) {
                    let _ = client.try_sweep_expired(&l.seller, &listing);
                }
            }
            8 => {
                if client
                    .try_make_collection_offer(buyer, &nft_addr, &amount, &(seq + ttl))
                    .is_ok()
                {
                    collection_offers += 1;
                }
            }
            9 => {
                let offer = u64::from(b) % (collection_offers + 1);
                let owner = nft.owner_of(&token);
                let expired = client
                    .get_collection_offer(&offer)
                    .is_some_and(|o| seq > o.expires_at);
                let res = client.try_accept_collection_offer(&owner, &offer, &token);
                if expired {
                    assert!(res.is_err(), "expired collection offer accepted");
                }
                sold = res.is_ok();
            }
            10 => {
                let offer = u64::from(b) % (collection_offers + 1);
                let _ = client.try_cancel_collection_offer(buyer, &offer);
            }
            _ => {
                env.ledger()
                    .with_mut(|l| l.sequence_number += u32::from(chunk[5] % 32));
            }
        }

        // Solvency.
        let held = tok.balance(&marketplace);
        assert_eq!(held, escrowed(listings, collection_offers), "escrow mismatch");

        // Conservation.
        let total: i128 = held + actors.iter().map(|x| tok.balance(x)).sum::<i128>();
        assert_eq!(total, total_supply, "payment tokens created or destroyed");

        // Custody.
        for (id, before) in (1..=TOKENS).zip(owners_before.iter()) {
            let after = nft.owner_of(&id);
            if &after != before {
                assert!(sold, "NFT moved without a successful sale");
                let idx = actors.iter().position(|x| x == before).unwrap();
                assert!(
                    tok.balance(before) > balances_before[idx],
                    "previous NFT owner was not paid"
                );
            }
        }
    }

    // No trapped funds: every buyer can recover every outstanding offer.
    for l in 0..listings {
        for b in &buyers {
            if client.get_offer(&l, b).is_some() {
                client.cancel_offer(b, &l);
            }
        }
    }
    for o in 0..collection_offers {
        if let Some(offer) = client.get_collection_offer(&o) {
            client.cancel_collection_offer(&offer.buyer, &o);
        }
    }
    assert_eq!(tok.balance(&marketplace), 0, "funds trapped in marketplace");
});
