#![no_std]
#![deny(missing_docs)]
//! Non-fungible token (NFT) contract template.
//!
//! Admin-controlled minting with per-token owners, approved spenders, metadata
//! URIs, and an optional supply cap.

use soroban_sdk::{Address, Env, String, Vec, contract, contractimpl};

mod errors;
mod events;
mod storage;

pub use errors::NftError;
pub use storage::{DataKey, RoyaltyInfo, TokenKey, TokenMetadata};

use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee, extend_ttl_persistent,
};

/// Maximum royalty that may be configured, in basis points (15%).
pub const MAX_ROYALTY_BPS: u32 = 1_500;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn balance(env: &Env, owner: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(owner.clone()))
        .unwrap_or(0)
}

fn set_balance(env: &Env, owner: &Address, amount: u32) {
    let key = DataKey::Balance(owner.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    }
}

fn is_operator(env: &Env, owner: &Address, operator: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::OperatorApproval(owner.clone(), operator.clone()))
        .unwrap_or(false)
}

/// Owner, single-token approved spender, or collection-wide operator.
fn is_authorized_spender(env: &Env, spender: &Address, owner: &Address, token_id: u32) -> bool {
    if spender == owner || is_operator(env, owner, spender) {
        return true;
    }
    env.storage()
        .instance()
        .get::<_, Address>(&TokenKey::Approval(token_id))
        .is_some_and(|approved| &approved == spender)
}

/// Move `token_id` from `from` to `to`, clearing any single-token approval and
/// updating balances. Caller must have already checked authorization.
fn move_token(env: &Env, from: &Address, to: &Address, token_id: u32) {
    env.storage()
        .instance()
        .set(&TokenKey::Owner(token_id), to);
    env.storage()
        .instance()
        .remove(&TokenKey::Approval(token_id));
    if from != to {
        set_balance(env, from, balance(env, from).saturating_sub(1));
        set_balance(env, to, balance(env, to).saturating_add(1));
    }
    bump_instance(env);
    events::transferred(env, from, to, token_id);
}

/// Non-fungible token (NFT) contract.
fn bump_token(env: &Env, key: &TokenKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_data(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn owner_balance(env: &Env, owner: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(owner.clone()))
        .unwrap_or(0)
}

fn enumerable_add(env: &Env, owner: &Address, token_id: u32) {
    let balance = owner_balance(env, owner);
    let tokens_key = DataKey::OwnerTokens(owner.clone(), balance);
    let index_key = DataKey::OwnedIndex(token_id);
    env.storage().persistent().set(&tokens_key, &token_id);
    env.storage().persistent().set(&index_key, &balance);
    env.storage()
        .persistent()
        .set(&DataKey::Balance(owner.clone()), &(balance.saturating_add(1)));
    bump_data(env, &tokens_key);
    bump_data(env, &index_key);
    bump_data(env, &DataKey::Balance(owner.clone()));
}

fn enumerable_remove(env: &Env, owner: &Address, token_id: u32) {
    let balance = owner_balance(env, owner);
    if balance == 0 {
        return;
    }
    let index_key = DataKey::OwnedIndex(token_id);
    let index: u32 = env.storage().persistent().get(&index_key).unwrap_or(0);
    let last_index = balance.saturating_sub(1);
    let last_key = DataKey::OwnerTokens(owner.clone(), last_index);
    if index != last_index {
        let last_token: u32 = env.storage().persistent().get(&last_key).unwrap_or(0);
        let swap_key = DataKey::OwnerTokens(owner.clone(), index);
        env.storage().persistent().set(&swap_key, &last_token);
        env.storage()
            .persistent()
            .set(&DataKey::OwnedIndex(last_token), &index);
        bump_data(env, &swap_key);
        bump_data(env, &DataKey::OwnedIndex(last_token));
    }
    env.storage().persistent().remove(&last_key);
    env.storage().persistent().remove(&index_key);
    env.storage()
        .persistent()
        .set(&DataKey::Balance(owner.clone()), &last_index);
    bump_data(env, &DataKey::Balance(owner.clone()));
}


/// Non-fungible token (NFT) contract with admin-controlled minting and optional supply cap.
///
/// Each token has a unique `token_id` (`u32`), an owner, an optional approved spender,
/// and a URI pointing to off-chain metadata.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct NftContract;

    #[contractimpl]
    impl NftContract {
        /// Initialize the NFT collection.
        ///
        /// `max_supply` of `0` means no cap.
        ///
        /// `royalty_bps` sets a collection-level default royalty in basis points
        /// (0–10 000). `royalty_recipient` is required when `royalty_bps > 0`.
        /// Both default to no royalty when omitted (`None`).
        ///
        /// # Errors
        ///
        /// Returns [`NftError::AlreadyInitialized`] if called a second time.
        /// Returns [`NftError::InvalidRoyalty`] if `royalty_bps > 10 000`.
        /// Returns [`NftError::RoyaltyRecipientMissing`] if `royalty_bps > 0` but
        /// `royalty_recipient` is `None`.
        pub fn initialize(
            env: Env,
            admin: Address,
            name: String,
            symbol: String,
            max_supply: u32,
            royalty_bps: Option<u32>,
            royalty_recipient: Option<Address>,
            soulbound: bool,
        ) -> Result<(), NftError> {
            if env.storage().instance().has(&DataKey::Initialized) {
                return Err(NftError::AlreadyInitialized);
            }

            // Validate collection-level royalty parameters.
            if let Some(bps) = royalty_bps {
                if bps > MAX_ROYALTY_BPS {
                    return Err(NftError::InvalidRoyalty);
                }
                if bps > 0 && royalty_recipient.is_none() {
                    return Err(NftError::RoyaltyRecipientMissing);
                }
            }

            admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Name, &name);
            env.storage().instance().set(&DataKey::Symbol, &symbol);
            env.storage().instance().set(&DataKey::TotalSupply, &0u32);
            if max_supply > 0 {
                env.storage()
                    .instance()
                    .set(&DataKey::MaxSupply, &max_supply);
            }
            if let Some(bps) = royalty_bps {
                env.storage().instance().set(&DataKey::RoyaltyBps, &bps);
            }
            if let Some(ref recipient) = royalty_recipient {
                env.storage()
                    .instance()
                    .set(&DataKey::RoyaltyRecipient, recipient);
            }
            env.storage().instance().set(&DataKey::NextTokenId, &1u32);
            env.storage().instance().set(&DataKey::Soulbound, &soulbound);
            env.storage().instance().set(&DataKey::Initialized, &true);

            bump_instance(&env);
            events::initialized(&env, &admin, &name, &symbol);

            Ok(())
        }

        /// Mint a new token to `to` with the given `token_id` and `token_uri`. Admin only.
        ///
        /// `token_royalty_bps` and `token_royalty_recipient` are optional per-token
        /// royalty overrides (EIP-2981-style). When set they take precedence over the
        /// collection-level defaults returned by [`royalty_info`]. Both must be `None`
        /// or both must be `Some` (with `token_royalty_bps` in 0–10 000).
        ///
        /// # Errors
        /// - [`NftError::NotInitialized`]
        /// - [`NftError::NotAuthorized`]
        /// - [`NftError::SupplyCapReached`]
        /// - [`NftError::InvalidRoyalty`] if `token_royalty_bps > 10 000`.
        /// - [`NftError::RoyaltyRecipientMissing`] if `token_royalty_bps > 0` but
        ///   `token_royalty_recipient` is `None`.
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::NotAuthorized`] if the caller is not the admin.
        /// Returns [`NftError::TokenAlreadyMinted`] if `token_id` is already taken.
        /// Returns [`NftError::SupplyCapReached`] if minting would exceed the max supply.
        /// Returns [`NftError::InvalidRoyalty`] if `token_royalty_bps > 10 000`.
        /// Returns [`NftError::RoyaltyRecipientMissing`] if `token_royalty_bps > 0` but
        /// `token_royalty_recipient` is `None`.
        pub fn mint(
            env: Env,
            to: Address,
            token_id: u32,
            token_uri: String,
            token_royalty_bps: Option<u32>,
            token_royalty_recipient: Option<Address>,
        ) -> Result<(), NftError> {
            Self::require_initialized(&env)?;

            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(NftError::NotInitialized)?;
            admin.require_auth();

            if env.storage().persistent().has(&TokenKey::Owner(token_id)) {
                return Err(NftError::TokenAlreadyMinted);
            }

            // Validate per-token royalty parameters.
            if let Some(bps) = token_royalty_bps {
                if bps > MAX_ROYALTY_BPS {
                    return Err(NftError::InvalidRoyalty);
                }
                if bps > 0 && token_royalty_recipient.is_none() {
                    return Err(NftError::RoyaltyRecipientMissing);
                }
            }

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            if let Some(max) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxSupply)
            {
                if total >= max {
                    return Err(NftError::SupplyCapReached);
                }
            }

            env.storage()
                .persistent()
                .set(&TokenKey::Owner(token_id), &to);
            env.storage()
                .persistent()
                .set(&TokenKey::Uri(token_id), &token_uri);
            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &(total + 1));
            enumerable_add(&env, &to, token_id);

            // Store per-token royalty overrides if provided.
            if let Some(bps) = token_royalty_bps {
                env.storage()
                    .persistent()
                    .set(&TokenKey::TokenRoyaltyBps(token_id), &bps);
                bump_token(&env, &TokenKey::TokenRoyaltyBps(token_id));
            }
            if let Some(ref recipient) = token_royalty_recipient {
                env.storage()
                    .persistent()
                    .set(&TokenKey::TokenRoyaltyRecipient(token_id), recipient);
                bump_token(&env, &TokenKey::TokenRoyaltyRecipient(token_id));
            }

            // Keep sequential allocator ahead of explicitly chosen ids.
            let next: u32 = env
                .storage()
                .instance()
                .get(&DataKey::NextTokenId)
                .unwrap_or(1);
            if token_id >= next {
                env.storage()
                    .instance()
                    .set(&DataKey::NextTokenId, &(token_id + 1));
            }

            set_balance(&env, &to, balance(&env, &to).saturating_add(1));

            bump_instance(&env);
            bump_token(&env, &TokenKey::Owner(token_id));
            bump_token(&env, &TokenKey::Uri(token_id));
            events::minted(&env, &to, token_id);

            Ok(())
        }

        /// Mint `count` NFTs to `to` with contiguous sequential id allocation.
        ///
        /// `uris` must contain exactly `count` metadata URIs. Token ids are
        /// assigned contiguously starting from the current `NextTokenId`, the
        /// counter is advanced once, and a single batch-mint event covering the
        /// id range is emitted.
        ///
        /// Returns the list of newly minted token ids in order.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::NotAuthorized`] if the caller is not the admin.
        /// Returns [`NftError::InvalidTokenId`] if `uris.len() != count`.
        /// Returns [`NftError::SupplyCapReached`] if minting would exceed the max supply.
        pub fn mint_batch(
            env: Env,
            to: Address,
            count: u32,
            uris: Vec<String>,
        ) -> Result<Vec<u32>, NftError> {
            Self::require_initialized(&env)?;

            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(NftError::NotInitialized)?;
            admin.require_auth();

            if uris.len() != count {
                return Err(NftError::InvalidTokenId);
            }
            if count == 0 {
                return Ok(Vec::new(&env));
            }

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            if let Some(max) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxSupply)
            {
                if total.saturating_add(count) > max {
                    return Err(NftError::SupplyCapReached);
                }
            }

            let mut next: u32 = env
                .storage()
                .instance()
                .get(&DataKey::NextTokenId)
                .unwrap_or(1);
            let mut minted = Vec::new(&env);
            let mut new_total = total;

            for uri in uris.iter() {
                // Skip any ids already taken by explicit mint.
                while env.storage().persistent().has(&TokenKey::Owner(next)) {
                    next = next.checked_add(1).ok_or(NftError::SupplyCapReached)?;
                }

                let token_id = next;
                next = next.checked_add(1).ok_or(NftError::SupplyCapReached)?;
                new_total = new_total.checked_add(1).ok_or(NftError::SupplyCapReached)?;

                env.storage()
                    .persistent()
                    .set(&TokenKey::Owner(token_id), &to);
                env.storage()
                    .persistent()
                    .set(&TokenKey::Uri(token_id), &uri);
                enumerable_add(&env, &to, token_id);
                bump_token(&env, &TokenKey::Owner(token_id));
                bump_token(&env, &TokenKey::Uri(token_id));
                minted.push_back(token_id);
            }

            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &new_total);
            env.storage()
                .instance()
                .set(&DataKey::NextTokenId, &next);
            bump_instance(&env);

            // Emit a single batch-mint event covering the contiguous id range.
            let first_id = minted.first().unwrap_or(0);
            let last_id = minted.last().unwrap_or(0);
            let topics = (soroban_sdk::symbol_short!("mint_batch"), to.clone());
            env.events()
                .publish(&topics, (first_id, last_id, count));

            Ok(minted)
        }

        /// Transfer multiple tokens from `from` to `to` in one call.
        ///
        /// Validates that `from` owns every token before applying any transfer.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if any token does not exist.
        /// Returns [`NftError::NotOwner`] if `from` does not own every token.
        pub fn transfer_batch(
            env: Env,
            from: Address,
            to: Address,
            token_ids: Vec<u32>,
        ) -> Result<(), NftError> {
            Self::require_initialized(&env)?;
            Self::require_transferable(&env)?;
            from.require_auth();

            // Validate ownership for all tokens first (all-or-nothing).
            for token_id in token_ids.iter() {
                let owner: Address = env
                    .storage()
                    .persistent()
                    .get(&TokenKey::Owner(token_id))
                    .ok_or(NftError::TokenNotFound)?;
                if owner != from {
                    return Err(NftError::NotOwner);
                }
            }

            for token_id in token_ids.iter() {
                enumerable_remove(&env, &from, token_id);
                env.storage()
                    .persistent()
                    .set(&TokenKey::Owner(token_id), &to);
                enumerable_add(&env, &to, token_id);
                env.storage()
                    .persistent()
                    .remove(&TokenKey::Approval(token_id));
                bump_token(&env, &TokenKey::Owner(token_id));
                events::transferred(&env, &from, &to, token_id);
            }
            Ok(())
        }

        /// Return royalty information for a token sale (EIP-2981-style view).
        ///
        /// Resolution order:
        /// 1. Per-token royalty BPS and recipient (set at mint time), if present.
        /// 2. Collection-level royalty BPS and recipient (set at initialize time).
        /// 3. If neither is configured, returns `None`.
        ///
        /// `sale_price` is the gross sale amount in base units. The returned
        /// [`RoyaltyInfo::amount`] is pre-computed as `sale_price * bps / 10_000`.
        ///
        /// Integrating marketplaces (e.g. this repo's own marketplace) should call
        /// this before executing a sale and route the indicated `amount` to the
        /// `recipient`.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if `token_id` does not exist.
        #[must_use]
        pub fn royalty_info(
            env: Env,
            token_id: u32,
            sale_price: i128,
        ) -> Result<Option<RoyaltyInfo>, NftError> {
            Self::require_initialized(&env)?;

            // Confirm the token exists.
            if !env.storage().persistent().has(&TokenKey::Owner(token_id)) {
                return Err(NftError::TokenNotFound);
            }

            // 1. Per-token override (bps may be 0 with no recipient).
            let token_bps: Option<u32> = env
                .storage()
                .persistent()
                .get(&TokenKey::TokenRoyaltyBps(token_id));
            let token_recipient: Option<Address> = env
                .storage()
                .persistent()
                .get(&TokenKey::TokenRoyaltyRecipient(token_id));

            if let Some(bps) = token_bps {
                if bps == 0 {
                    return Ok(None);
                }
                let recipient = token_recipient.ok_or(NftError::RoyaltyRecipientMissing)?;
                let amount = apply_bps_fee(sale_price, bps).unwrap_or(0);
                return Ok(Some(RoyaltyInfo { recipient, amount }));
            }

            // 2. Collection-level defaults.
            let collection_bps: Option<u32> =
                env.storage().instance().get(&DataKey::RoyaltyBps);
            let collection_recipient: Option<Address> =
                env.storage().instance().get(&DataKey::RoyaltyRecipient);

            if let (Some(bps), Some(recipient)) = (collection_bps, collection_recipient) {
                if bps == 0 {
                    return Ok(None);
                }
                let amount = apply_bps_fee(sale_price, bps).unwrap_or(0);
                return Ok(Some(RoyaltyInfo { recipient, amount }));
            }

            // 3. No royalty configured.
            Ok(None)
        }

        /// Transfer token `token_id` from `from` to `to`. Requires auth from `from` (the owner).
        ///
        /// # Errors
        /// - [`NftError::NotInitialized`]
        /// - [`NftError::TokenNotFound`]
        /// - [`NftError::NotAuthorized`]
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NotOwner`] if `from` is not the current owner.
        pub fn transfer(
            env: Env,
            from: Address,
            to: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            Self::require_initialized(&env)?;
            Self::require_transferable(&env)?;
            from.require_auth();

            let owner: Address = env
                .storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;

            if owner != from {
                return Err(NftError::NotAuthorized);
            }

            move_token(&env, &from, &to, token_id);
            Ok(())
        }

        /// Transfer `token_id` from `from` to `to` on behalf of the owner.
        ///
        /// `spender` must authorize the call and be the owner, the approved
        /// spender for `token_id`, or an operator approved via
        /// [`set_approval_for_all`]. Any single-token approval is cleared.
        ///
        /// # Errors
        /// - [`NftError::TokenNotFound`]
        /// - [`NftError::NotOwner`] if `from` does not own `token_id`
        /// - [`NftError::NotApproved`] if `spender` is not authorized
        pub fn transfer_from(
            env: Env,
            spender: Address,
            from: Address,
            to: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            spender.require_auth();

            let owner: Address = env
                .storage()
                .instance()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;
            if owner != from {
                return Err(NftError::NotOwner);
            }
            if !is_authorized_spender(&env, &spender, &owner, token_id) {
                return Err(NftError::NotApproved);
            }

            move_token(&env, &from, &to, token_id);
            Ok(())
        }

        /// Grant or revoke `operator` permission to transfer and burn all of
        /// `owner`'s tokens. Requires auth from `owner`.
        pub fn set_approval_for_all(env: Env, owner: Address, operator: Address, approved: bool) {
            owner.require_auth();

            let key = DataKey::OperatorApproval(owner.clone(), operator.clone());
            if approved {
                env.storage().persistent().set(&key, &true);
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            } else {
                env.storage().persistent().remove(&key);
            }
            events::approval_for_all(&env, &owner, &operator, approved);
        }

        /// Whether `operator` is approved to manage all of `owner`'s tokens.
        pub fn is_approved_for_all(env: Env, owner: Address, operator: Address) -> bool {
            is_operator(&env, &owner, &operator)
        }

        /// Permanently destroy `token_id`.
        ///
        /// `caller` must authorize the call and be the owner, the approved
        /// spender, or an approved operator. Removes owner, URI, approval and
        /// royalty data, decrements the owner's balance and the active supply.
        ///
        /// # Errors
        /// - [`NftError::TokenNotFound`]
        /// - [`NftError::NotApproved`] if `caller` is not authorized
        pub fn burn(env: Env, caller: Address, token_id: u32) -> Result<(), NftError> {
            caller.require_auth();

            let owner: Address = env
                .storage()
                .instance()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;
            if !is_authorized_spender(&env, &caller, &owner, token_id) {
                return Err(NftError::NotApproved);
            }

            let instance = env.storage().instance();
            instance.remove(&TokenKey::Owner(token_id));
            instance.remove(&TokenKey::Uri(token_id));
            instance.remove(&TokenKey::Approval(token_id));
            instance.remove(&TokenKey::TokenRoyaltyBps(token_id));
            instance.remove(&TokenKey::TokenRoyaltyRecipient(token_id));
            env.storage()
                .persistent()
                .remove(&TokenKey::TokenRoyaltyRecipient(token_id));

            set_balance(&env, &owner, balance(&env, &owner).saturating_sub(1));
            let burned: u32 = instance.get(&DataKey::Burned).unwrap_or(0);
            instance.set(&DataKey::Burned, &burned.saturating_add(1));

            bump_instance(&env);
            events::burned(&env, &caller, token_id);
            Ok(())
        }

        /// Number of tokens currently owned by `owner`.
        pub fn balance_of(env: Env, owner: Address) -> u32 {
            balance(&env, &owner)
        }

        /// Approve `spender` to transfer or sell token `token_id` on behalf of the owner.
        ///
        /// # Errors
        /// - [`NftError::NotInitialized`]
        /// - [`NftError::TokenNotFound`]
        /// - [`NftError::NotAuthorized`]
        pub fn approve(
            env: Env,
            owner: Address,
            spender: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            owner.require_auth();

            env.storage()
                .persistent()
                .set(&TokenKey::Approval(token_id), &spender);
            bump_token(&env, &TokenKey::Approval(token_id));
            events::approved(&env, &owner, &spender, token_id);

            Ok(())
        }


        /// Burn (destroy) token `token_id`. Requires auth from the current owner.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NotOwner`] if `from` is not the current owner.
        pub fn burn(env: Env, from: Address, token_id: u32) -> Result<(), NftError> {
            Self::require_initialized(&env)?;
            from.require_auth();

            let owner: Address = env
                .storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;
            if owner != from {
                return Err(NftError::NotOwner);
            }

            enumerable_remove(&env, &from, token_id);
            env.storage()
                .persistent()
                .remove(&TokenKey::Owner(token_id));
            env.storage().persistent().remove(&TokenKey::Uri(token_id));
            env.storage()
                .persistent()
                .remove(&TokenKey::Approval(token_id));

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(1);
            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &total.saturating_sub(1));

            bump_instance(&env);
            events::burned(&env, &from, token_id);

            Ok(())
        }

        /// Grant `spender` approval to transfer token `token_id`. Caller must be the owner.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NotOwner`] if the caller is not the current owner.
        pub fn approve(env: Env, token_id: u32, spender: Address) -> Result<(), NftError> {
            Self::require_initialized(&env)?;

            let owner: Address = env
                .storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;

            owner.require_auth();

            env.storage()
                .persistent()
                .set(&TokenKey::Approval(token_id), &spender);
            bump_token(&env, &TokenKey::Approval(token_id));
            events::approved(&env, &owner, &spender, token_id);

            Ok(())
        }

        /// Transfer token `token_id` from `from` to `to` using a prior approval. Auth from `spender`.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NotOwner`] if `from` is not the current owner.
        /// Returns [`NftError::NotApproved`] if `spender` is not approved for this token.
        pub fn transfer_from(
            env: Env,
            spender: Address,
            from: Address,
            to: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            Self::require_initialized(&env)?;
            Self::require_transferable(&env)?;
            spender.require_auth();

            let owner: Address = env
                .storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;

            if owner != from {
                return Err(NftError::NotOwner);
            }

            let approved: Option<Address> = env
                .storage()
                .persistent()
                .get(&TokenKey::Approval(token_id));

            match approved {
                Some(ref a) if *a == spender => {}
                _ => return Err(NftError::NotApproved),
            }

            enumerable_remove(&env, &from, token_id);
            env.storage()
                .persistent()
                .set(&TokenKey::Owner(token_id), &to);
            enumerable_add(&env, &to, token_id);
            env.storage()
                .persistent()
                .remove(&TokenKey::Approval(token_id));

            bump_token(&env, &TokenKey::Owner(token_id));
            events::transferred(&env, &from, &to, token_id);

            Ok(())
        }

        /// Return the owner of `token_id`.
        #[must_use]
        pub fn owner_of(env: Env, token_id: u32) -> Result<Address, NftError> {
            env.storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)
        }

        /// Return the approved spender for `token_id`, if any.
        #[must_use]
        pub fn get_approved(env: Env, token_id: u32) -> Option<Address> {
            env.storage()
                .persistent()
                .get(&TokenKey::Approval(token_id))
        }

        /// Return the token URI for `token_id`.
        #[must_use]
        pub fn token_uri(env: Env, token_id: u32) -> Result<String, NftError> {
            env.storage()
                .persistent()
                .get(&TokenKey::Uri(token_id))
                .ok_or(NftError::TokenNotFound)
        }

        /// Return collection metadata as a [`TokenMetadata`] struct.
        #[must_use]
        pub fn metadata(env: Env) -> Result<TokenMetadata, NftError> {
            Self::require_initialized(&env)?;
            Ok(TokenMetadata {
                name: env
                    .storage()
                    .instance()
                    .get(&DataKey::Name)
                    .ok_or(NftError::NotInitialized)?,
                symbol: env
                    .storage()
                    .instance()
                    .get(&DataKey::Symbol)
                    .ok_or(NftError::NotInitialized)?,
                token_uri: String::from_str(&env, ""),
            })
        }

        /// Return the collection name.
        #[must_use]
        pub fn name(env: Env) -> Result<String, NftError> {
            env.storage()
                .instance()
                .get(&DataKey::Name)
                .ok_or(NftError::NotInitialized)
        }

        /// Return the collection symbol.
        #[must_use]
        pub fn symbol(env: Env) -> Result<String, NftError> {
            env.storage()
                .instance()
                .get(&DataKey::Symbol)
                .ok_or(NftError::NotInitialized)
        }


        /// Return how many tokens `owner` currently holds.
        #[must_use]
        pub fn balance_of(env: Env, owner: Address) -> u32 {
            owner_balance(&env, &owner)
        }

        /// Return the token id at `index` in `owner`'s enumerable list.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::InvalidTokenId`] if `index` is out of range.
        pub fn token_of_owner_by_index(
            env: Env,
            owner: Address,
            index: u32,
        ) -> Result<u32, NftError> {
            let balance = owner_balance(&env, &owner);
            if index >= balance {
                return Err(NftError::InvalidTokenId);
            }
            env.storage()
                .persistent()
                .get(&DataKey::OwnerTokens(owner, index))
                .ok_or(NftError::InvalidTokenId)
        }

        /// Return whether this collection is soulbound (non-transferable).
        #[must_use]
        pub fn is_soulbound(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&DataKey::Soulbound)
                .unwrap_or(false)
        }

        /// Return the total number of minted (non-burned) tokens.
        #[must_use]
        pub fn total_supply(env: Env) -> u32 {
            let minted: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            let burned: u32 = env.storage().instance().get(&DataKey::Burned).unwrap_or(0);
            minted.saturating_sub(burned)
        }


        fn require_transferable(env: &Env) -> Result<(), NftError> {
            let soulbound: bool = env
                .storage()
                .instance()
                .get(&DataKey::Soulbound)
                .unwrap_or(false);
            if soulbound {
                return Err(NftError::NonTransferable);
            }
            Ok(())
        }

        fn require_initialized(env: &Env) -> Result<(), NftError> {
            if !env.storage().instance().has(&DataKey::Initialized) {
                return Err(NftError::NotInitialized);
            }
            Ok(())
        }
    }
}

mod test;
#[cfg(test)]
mod royalty_test;
