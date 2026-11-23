#![no_std]
#![deny(missing_docs)]
//! Non-fungible token (NFT) contract template.
//!
//! Admin-controlled minting with per-token owners, approved spenders, metadata
//! URIs, and an optional supply cap.
//!
//! # Enumerable ownership index (issue #1474)
//! Every owner has a dense, zero-based list of the token ids they hold
//! (`DataKey::OwnerTokens(owner, index)`), plus a reverse lookup from token id
//! to its position (`DataKey::OwnedIndex(token_id)`) and a per-owner count
//! (`DataKey::Balance(owner)`). The index is updated on mint, transfer,
//! `transfer_from`, `transfer_batch` and burn. Removals use swap-and-pop: the
//! owner's last token moves into the freed slot, so the list never has gaps and
//! [`NftContract::balance_of`] always equals the list length.
//!
//! # Soulbound tokens (issues #1188, #1475)
//! A whole collection can be made soulbound at `initialize`, and individual
//! tokens can be minted soulbound with [`NftContract::mint_with_soulbound`].
//! Soulbound tokens can be minted and burned, but `transfer`, `transfer_from`
//! and `transfer_batch` revert with [`NftError::NonTransferable`].

use soroban_sdk::{Address, Env, String, Vec, contract, contractimpl};

mod errors;
mod events;
mod storage;

pub use errors::NftError;
pub use storage::{DataKey, RoyaltyInfo, TokenKey, TokenMetadata};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee};

/// Maximum royalty that may be configured, in basis points (15%).
pub const MAX_ROYALTY_BPS: u32 = 1_500;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

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

fn load_owner(env: &Env, token_id: u32) -> Result<Address, NftError> {
    env.storage()
        .persistent()
        .get(&TokenKey::Owner(token_id))
        .ok_or(NftError::TokenNotFound)
}

fn owner_balance(env: &Env, owner: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(owner.clone()))
        .unwrap_or(0)
}

fn set_owner_balance(env: &Env, owner: &Address, balance: u32) {
    let key = DataKey::Balance(owner.clone());
    if balance == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &balance);
        bump_data(env, &key);
    }
}

/// Append `token_id` to the end of `owner`'s enumerable list.
fn enumerable_add(env: &Env, owner: &Address, token_id: u32) -> Result<(), NftError> {
    let balance = owner_balance(env, owner);
    let new_balance = balance.checked_add(1).ok_or(NftError::SupplyCapReached)?;

    let tokens_key = DataKey::OwnerTokens(owner.clone(), balance);
    let index_key = DataKey::OwnedIndex(token_id);
    env.storage().persistent().set(&tokens_key, &token_id);
    env.storage().persistent().set(&index_key, &balance);
    bump_data(env, &tokens_key);
    bump_data(env, &index_key);
    set_owner_balance(env, owner, new_balance);
    Ok(())
}

/// Remove `token_id` from `owner`'s enumerable list with swap-and-pop.
///
/// The owner's last token is moved into the removed token's slot, so the list
/// stays dense and every remaining token's `OwnedIndex` stays correct.
fn enumerable_remove(env: &Env, owner: &Address, token_id: u32) -> Result<(), NftError> {
    let balance = owner_balance(env, owner);
    let last_index = balance.checked_sub(1).ok_or(NftError::NotOwner)?;

    let index_key = DataKey::OwnedIndex(token_id);
    let index: u32 = env
        .storage()
        .persistent()
        .get(&index_key)
        .ok_or(NftError::TokenNotFound)?;
    let last_key = DataKey::OwnerTokens(owner.clone(), last_index);

    if index != last_index {
        let last_token: u32 = env
            .storage()
            .persistent()
            .get(&last_key)
            .ok_or(NftError::TokenNotFound)?;
        let slot_key = DataKey::OwnerTokens(owner.clone(), index);
        let moved_index_key = DataKey::OwnedIndex(last_token);
        env.storage().persistent().set(&slot_key, &last_token);
        env.storage().persistent().set(&moved_index_key, &index);
        bump_data(env, &slot_key);
        bump_data(env, &moved_index_key);
    }

    env.storage().persistent().remove(&last_key);
    env.storage().persistent().remove(&index_key);
    set_owner_balance(env, owner, last_index);
    Ok(())
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
        .persistent()
        .get::<_, Address>(&TokenKey::Approved(token_id))
        .is_some_and(|approved| &approved == spender)
}

fn collection_soulbound(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Soulbound)
        .unwrap_or(false)
}

/// Whether `token_id` is soulbound, either individually or via its collection.
fn token_soulbound(env: &Env, token_id: u32) -> bool {
    collection_soulbound(env)
        || env
            .storage()
            .persistent()
            .get(&TokenKey::Soulbound(token_id))
            .unwrap_or(false)
}

fn require_transferable(env: &Env, token_id: u32) -> Result<(), NftError> {
    if token_soulbound(env, token_id) {
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

fn require_admin(env: &Env) -> Result<(), NftError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(NftError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn validate_royalty(bps: Option<u32>, recipient: &Option<Address>) -> Result<(), NftError> {
    if let Some(bps) = bps {
        if bps > MAX_ROYALTY_BPS {
            return Err(NftError::InvalidRoyalty);
        }
        if bps > 0 && recipient.is_none() {
            return Err(NftError::RoyaltyRecipientMissing);
        }
    }
    Ok(())
}

fn current_supply(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0)
}

/// Fail with `SupplyCapReached` if minting `count` more tokens would exceed the cap.
fn check_supply_cap(env: &Env, count: u32) -> Result<u32, NftError> {
    let new_total = current_supply(env)
        .checked_add(count)
        .ok_or(NftError::SupplyCapReached)?;
    if let Some(max) = env
        .storage()
        .instance()
        .get::<DataKey, u32>(&DataKey::MaxSupply)
    {
        if new_total > max {
            return Err(NftError::SupplyCapReached);
        }
    }
    Ok(new_total)
}

/// Write a freshly minted token's owner, URI and index entries.
fn store_new_token(
    env: &Env,
    to: &Address,
    token_id: u32,
    token_uri: &String,
    soulbound: bool,
) -> Result<(), NftError> {
    env.storage()
        .persistent()
        .set(&TokenKey::Owner(token_id), to);
    env.storage()
        .persistent()
        .set(&TokenKey::Uri(token_id), token_uri);
    bump_token(env, &TokenKey::Owner(token_id));
    bump_token(env, &TokenKey::Uri(token_id));
    if soulbound {
        env.storage()
            .persistent()
            .set(&TokenKey::Soulbound(token_id), &true);
        bump_token(env, &TokenKey::Soulbound(token_id));
    }
    enumerable_add(env, to, token_id)
}

/// Move `token_id` from `from` to `to`, clearing any single-token approval and
/// keeping the enumerable index in sync. Callers check ownership, authorization
/// and transferability first.
fn move_token(env: &Env, from: &Address, to: &Address, token_id: u32) -> Result<(), NftError> {
    if from != to {
        enumerable_remove(env, from, token_id)?;
        env.storage()
            .persistent()
            .set(&TokenKey::Owner(token_id), to);
        enumerable_add(env, to, token_id)?;
    }
    env.storage()
        .persistent()
        .remove(&TokenKey::Approved(token_id));
    bump_token(env, &TokenKey::Owner(token_id));
    events::transferred(env, from, to, token_id);
    Ok(())
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
        /// (0–[`MAX_ROYALTY_BPS`]). `royalty_recipient` is required when
        /// `royalty_bps > 0`. Both default to no royalty when omitted (`None`).
        ///
        /// `soulbound` makes every token in the collection non-transferable.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::AlreadyInitialized`] if called a second time.
        /// Returns [`NftError::InvalidRoyalty`] if `royalty_bps > MAX_ROYALTY_BPS`.
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
            validate_royalty(royalty_bps, &royalty_recipient)?;

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

        /// Mint a new, transferable token to `to` with the given `token_id` and
        /// `token_uri`. Admin only. Equivalent to
        /// [`NftContract::mint_with_soulbound`] with `soulbound = false`.
        ///
        /// `token_royalty_bps` and `token_royalty_recipient` are optional per-token
        /// royalty overrides (EIP-2981-style). When set they take precedence over the
        /// collection-level defaults returned by `royalty_info`.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenAlreadyMinted`] if `token_id` is already taken.
        /// Returns [`NftError::SupplyCapReached`] if minting would exceed the max supply.
        /// Returns [`NftError::InvalidRoyalty`] if `token_royalty_bps > MAX_ROYALTY_BPS`.
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
            Self::mint_with_soulbound(
                env,
                to,
                token_id,
                token_uri,
                token_royalty_bps,
                token_royalty_recipient,
                false,
            )
        }

        /// Mint a new token to `to`, optionally marking it soulbound (issue #1475).
        /// Admin only.
        ///
        /// A soulbound token stays with its first owner: `transfer`,
        /// `transfer_from` and `transfer_batch` revert with
        /// [`NftError::NonTransferable`]. It can still be burned.
        ///
        /// # Errors
        ///
        /// Same as [`NftContract::mint`].
        pub fn mint_with_soulbound(
            env: Env,
            to: Address,
            token_id: u32,
            token_uri: String,
            token_royalty_bps: Option<u32>,
            token_royalty_recipient: Option<Address>,
            soulbound: bool,
        ) -> Result<(), NftError> {
            require_initialized(&env)?;
            require_admin(&env)?;

            if env.storage().persistent().has(&TokenKey::Owner(token_id)) {
                return Err(NftError::TokenAlreadyMinted);
            }
            validate_royalty(token_royalty_bps, &token_royalty_recipient)?;
            let new_total = check_supply_cap(&env, 1)?;

            store_new_token(&env, &to, token_id, &token_uri, soulbound)?;
            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &new_total);

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

            // Keep the sequential allocator ahead of explicitly chosen ids.
            let next: u32 = env
                .storage()
                .instance()
                .get(&DataKey::NextTokenId)
                .unwrap_or(1);
            if token_id >= next {
                let after = token_id.checked_add(1).ok_or(NftError::InvalidTokenId)?;
                env.storage().instance().set(&DataKey::NextTokenId, &after);
            }

            bump_instance(&env);
            events::minted(&env, &to, token_id);

            Ok(())
        }

        /// Mint one transferable NFT per entry in `uris` to `to`, with contiguous
        /// sequential id allocation. Admin only.
        ///
        /// Ids are assigned from the current `NextTokenId`, skipping ids already
        /// taken by an explicit `mint`. A single `batch_minted` event covering the
        /// batch is emitted. Returns the newly minted token ids in order.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::SupplyCapReached`] if minting would exceed the max supply.
        pub fn mint_batch(env: Env, to: Address, uris: Vec<String>) -> Result<Vec<u32>, NftError> {
            require_initialized(&env)?;
            require_admin(&env)?;

            let count = uris.len();
            if count == 0 {
                return Ok(Vec::new(&env));
            }
            let new_total = check_supply_cap(&env, count)?;

            let mut next: u32 = env
                .storage()
                .instance()
                .get(&DataKey::NextTokenId)
                .unwrap_or(1);
            let mut minted = Vec::new(&env);

            for uri in uris.iter() {
                // Skip any ids already taken by explicit mint.
                while env.storage().persistent().has(&TokenKey::Owner(next)) {
                    next = next.checked_add(1).ok_or(NftError::SupplyCapReached)?;
                }
                let token_id = next;
                next = next.checked_add(1).ok_or(NftError::SupplyCapReached)?;

                store_new_token(&env, &to, token_id, &uri, false)?;
                minted.push_back(token_id);
            }

            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &new_total);
            env.storage().instance().set(&DataKey::NextTokenId, &next);
            bump_instance(&env);

            let first_id = minted.first().unwrap_or(0);
            events::batch_minted(&env, &to, first_id, count);

            Ok(minted)
        }

        /// Transfer token `token_id` from `from` to `to`. Requires auth from `from`
        /// (the owner).
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NonTransferable`] if the token is soulbound.
        /// Returns [`NftError::NotOwner`] if `from` is not the current owner.
        pub fn transfer(
            env: Env,
            from: Address,
            to: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            require_initialized(&env)?;
            from.require_auth();

            let owner = load_owner(&env, token_id)?;
            require_transferable(&env, token_id)?;
            if owner != from {
                return Err(NftError::NotOwner);
            }

            move_token(&env, &from, &to, token_id)?;
            bump_instance(&env);
            Ok(())
        }

        /// Transfer `token_id` from `from` to `to` on behalf of the owner.
        ///
        /// `spender` must authorize the call and be the owner, the approved
        /// spender for `token_id`, or an operator approved via
        /// `set_approval_for_all`. Any single-token approval is cleared.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NonTransferable`] if the token is soulbound.
        /// Returns [`NftError::NotOwner`] if `from` does not own `token_id`.
        /// Returns [`NftError::NotApproved`] if `spender` is not authorized.
        pub fn transfer_from(
            env: Env,
            spender: Address,
            from: Address,
            to: Address,
            token_id: u32,
        ) -> Result<(), NftError> {
            require_initialized(&env)?;
            spender.require_auth();

            let owner = load_owner(&env, token_id)?;
            require_transferable(&env, token_id)?;
            if owner != from {
                return Err(NftError::NotOwner);
            }
            if !is_authorized_spender(&env, &spender, &owner, token_id) {
                return Err(NftError::NotApproved);
            }

            move_token(&env, &from, &to, token_id)?;
            bump_instance(&env);
            Ok(())
        }

        /// Transfer multiple tokens from `from` to `to` in one call.
        ///
        /// Validates that every token exists, is transferable, and is owned by
        /// `from` before applying any transfer (all-or-nothing).
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if any token does not exist.
        /// Returns [`NftError::NonTransferable`] if any token is soulbound.
        /// Returns [`NftError::NotOwner`] if `from` does not own every token (including
        /// a token id listed twice).
        pub fn transfer_batch(
            env: Env,
            from: Address,
            to: Address,
            token_ids: Vec<u32>,
        ) -> Result<(), NftError> {
            require_initialized(&env)?;
            from.require_auth();

            for token_id in token_ids.iter() {
                let owner = load_owner(&env, token_id)?;
                require_transferable(&env, token_id)?;
                if owner != from {
                    return Err(NftError::NotOwner);
                }
            }

            for token_id in token_ids.iter() {
                // Re-check so a duplicated id cannot be moved twice and corrupt
                // the index; any failure reverts the whole batch.
                if load_owner(&env, token_id)? != from {
                    return Err(NftError::NotOwner);
                }
                move_token(&env, &from, &to, token_id)?;
            }
            bump_instance(&env);
            Ok(())
        }

        /// Grant `spender` approval to transfer token `token_id`. Caller must be the owner.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        pub fn approve(env: Env, token_id: u32, spender: Address) -> Result<(), NftError> {
            require_initialized(&env)?;

            let owner = load_owner(&env, token_id)?;
            owner.require_auth();

            env.storage()
                .persistent()
                .set(&TokenKey::Approved(token_id), &spender);
            bump_token(&env, &TokenKey::Approved(token_id));
            events::approved(&env, &owner, &spender, token_id);

            Ok(())
        }

        /// Grant or revoke `operator` permission to transfer and burn all of
        /// `owner`'s tokens. Requires auth from `owner`.
        pub fn set_approval_for_all(env: Env, owner: Address, operator: Address, approved: bool) {
            owner.require_auth();

            let key = DataKey::OperatorApproval(owner.clone(), operator.clone());
            if approved {
                env.storage().persistent().set(&key, &true);
                bump_data(&env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
            events::approval_for_all(&env, &owner, &operator, approved);
        }

        /// Whether `operator` is approved to manage all of `owner`'s tokens.
        pub fn is_approved_for_all(env: Env, owner: Address, operator: Address) -> bool {
            is_operator(&env, &owner, &operator)
        }

        /// Permanently destroy `token_id`. Soulbound tokens may be burned.
        ///
        /// `caller` must authorize the call and be the owner, the approved
        /// spender, or an approved operator. Removes the token's owner, URI,
        /// approval, soulbound and royalty data, removes it from the owner's
        /// enumerable list, and decrements the supply.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        /// Returns [`NftError::NotApproved`] if `caller` is not authorized.
        pub fn burn(env: Env, caller: Address, token_id: u32) -> Result<(), NftError> {
            require_initialized(&env)?;
            caller.require_auth();

            let owner = load_owner(&env, token_id)?;
            if !is_authorized_spender(&env, &caller, &owner, token_id) {
                return Err(NftError::NotApproved);
            }

            enumerable_remove(&env, &owner, token_id)?;
            let persistent = env.storage().persistent();
            persistent.remove(&TokenKey::Owner(token_id));
            persistent.remove(&TokenKey::Uri(token_id));
            persistent.remove(&TokenKey::Approved(token_id));
            persistent.remove(&TokenKey::Soulbound(token_id));
            persistent.remove(&TokenKey::TokenRoyaltyBps(token_id));
            persistent.remove(&TokenKey::TokenRoyaltyRecipient(token_id));

            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &current_supply(&env).saturating_sub(1));

            bump_instance(&env);
            events::burned(&env, &owner, token_id);
            Ok(())
        }

        /// Return royalty information for a token sale (EIP-2981-style view).
        ///
        /// Resolution order:
        /// 1. Per-token royalty BPS and recipient (set at mint time), if present.
        /// 2. Collection-level royalty BPS and recipient (set at initialize time).
        /// 3. If neither is configured, returns `None`.
        ///
        /// The returned [`RoyaltyInfo::amount`] is `sale_price * bps / 10_000`.
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
            require_initialized(&env)?;
            load_owner(&env, token_id)?;

            // 1. Per-token override (bps may be 0 with no recipient).
            let token_bps: Option<u32> = env
                .storage()
                .persistent()
                .get(&TokenKey::TokenRoyaltyBps(token_id));
            if let Some(bps) = token_bps {
                if bps == 0 {
                    return Ok(None);
                }
                let recipient: Address = env
                    .storage()
                    .persistent()
                    .get(&TokenKey::TokenRoyaltyRecipient(token_id))
                    .ok_or(NftError::RoyaltyRecipientMissing)?;
                let amount = apply_bps_fee(sale_price, bps).unwrap_or(0);
                return Ok(Some(RoyaltyInfo { recipient, amount }));
            }

            // 2. Collection-level defaults.
            let collection_bps: Option<u32> = env.storage().instance().get(&DataKey::RoyaltyBps);
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

        /// Return the owner of `token_id`.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        #[must_use]
        pub fn owner_of(env: Env, token_id: u32) -> Result<Address, NftError> {
            load_owner(&env, token_id)
        }

        /// Return the approved spender for `token_id`, if any.
        #[must_use]
        pub fn get_approved(env: Env, token_id: u32) -> Option<Address> {
            env.storage()
                .persistent()
                .get(&TokenKey::Approved(token_id))
        }

        /// Return the token URI for `token_id`.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if the token does not exist.
        #[must_use]
        pub fn token_uri(env: Env, token_id: u32) -> Result<String, NftError> {
            env.storage()
                .persistent()
                .get(&TokenKey::Uri(token_id))
                .ok_or(NftError::TokenNotFound)
        }

        /// Return collection metadata as a [`TokenMetadata`] struct.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        #[must_use]
        pub fn metadata(env: Env) -> Result<TokenMetadata, NftError> {
            require_initialized(&env)?;
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
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        #[must_use]
        pub fn name(env: Env) -> Result<String, NftError> {
            env.storage()
                .instance()
                .get(&DataKey::Name)
                .ok_or(NftError::NotInitialized)
        }

        /// Return the collection symbol.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::NotInitialized`] if the contract has not been set up.
        #[must_use]
        pub fn symbol(env: Env) -> Result<String, NftError> {
            env.storage()
                .instance()
                .get(&DataKey::Symbol)
                .ok_or(NftError::NotInitialized)
        }

        /// Return how many tokens `owner` currently holds.
        ///
        /// Always equals the length of `owner`'s enumerable list, so valid
        /// indexes for [`NftContract::tokens_by_owner`] are `0..balance_of(owner)`.
        #[must_use]
        pub fn balance_of(env: Env, owner: Address) -> u32 {
            owner_balance(&env, &owner)
        }

        /// Return the token id at position `index` in `owner`'s enumerable list
        /// (issue #1474). Order is not stable across transfers and burns
        /// (swap-and-pop), but every owned token appears exactly once.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::InvalidTokenId`] if `index >= balance_of(owner)`.
        pub fn tokens_by_owner(env: Env, owner: Address, index: u32) -> Result<u32, NftError> {
            if index >= owner_balance(&env, &owner) {
                return Err(NftError::InvalidTokenId);
            }
            env.storage()
                .persistent()
                .get(&DataKey::OwnerTokens(owner, index))
                .ok_or(NftError::InvalidTokenId)
        }

        /// Alias of [`NftContract::tokens_by_owner`] using the ERC-721 Enumerable name.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::InvalidTokenId`] if `index >= balance_of(owner)`.
        pub fn token_of_owner_by_index(
            env: Env,
            owner: Address,
            index: u32,
        ) -> Result<u32, NftError> {
            Self::tokens_by_owner(env, owner, index)
        }

        /// Return whether this whole collection is soulbound (non-transferable).
        #[must_use]
        pub fn is_soulbound(env: Env) -> bool {
            collection_soulbound(&env)
        }

        /// Return whether `token_id` is soulbound, either because it was minted
        /// soulbound or because the whole collection is.
        #[must_use]
        pub fn is_token_soulbound(env: Env, token_id: u32) -> bool {
            token_soulbound(&env, token_id)
        }

        /// Return the number of tokens currently in existence (minted minus burned).
        #[must_use]
        pub fn total_supply(env: Env) -> u32 {
            current_supply(&env)
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod royalty_test;

#[cfg(test)]
mod enumerable_soulbound_test;
