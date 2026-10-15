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

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

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
        ) -> Result<(), NftError> {
            if env.storage().instance().has(&DataKey::Initialized) {
                return Err(NftError::AlreadyInitialized);
            }

            // Validate collection-level royalty parameters.
            if let Some(bps) = royalty_bps {
                if bps > 10_000 {
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
                if bps > 10_000 {
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

            bump_instance(&env);
            bump_token(&env, &TokenKey::Owner(token_id));
            bump_token(&env, &TokenKey::Uri(token_id));
            events::minted(&env, &to, token_id);

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
                #[allow(
                    clippy::arithmetic_side_effects,
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    clippy::integer_division
                )]
                let amount = (sale_price * bps as i128) / 10_000;
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
                #[allow(clippy::arithmetic_side_effects, clippy::as_conversions, clippy::cast_possible_truncation, clippy::integer_division)]
                let amount = (sale_price * bps as i128) / 10_000;
                return Ok(Some(RoyaltyInfo { recipient, amount }));
            }

            // 3. No royalty configured.
            Ok(None)
        }

        /// Transfer token `token_id` from `from` to `to`. Requires auth from `from` (the owner).
        ///
        /// # Errors
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
            from.require_auth();

            let owner: Address = env
                .storage()
                .persistent()
                .get(&TokenKey::Owner(token_id))
                .ok_or(NftError::TokenNotFound)?;

            if owner != from {
                return Err(NftError::NotOwner);
            }

            env.storage()
                .persistent()
                .set(&TokenKey::Owner(token_id), &to);
            // Clear any existing approval on transfer.
            env.storage()
                .persistent()
                .remove(&TokenKey::Approval(token_id));

            bump_token(&env, &TokenKey::Owner(token_id));
            events::transferred(&env, &from, &to, token_id);

            Ok(())
        }

        /// Transfer multiple tokens from `from` to `to` in a single call. Requires a single
        /// auth from `from` (the owner) covering the whole batch.
        ///
        /// Ownership of every token in `token_ids` is validated before any transfer is applied,
        /// so the batch is atomic: either all tokens move or none do.
        ///
        /// # Errors
        ///
        /// Returns [`NftError::TokenNotFound`] if any token in `token_ids` does not exist.
        /// Returns [`NftError::NotOwner`] if `from` does not own any token in `token_ids`.
        pub fn batch_transfer(
            env: Env,
            from: Address,
            to: Address,
            token_ids: Vec<u32>,
        ) -> Result<(), NftError> {
            Self::require_initialized(&env)?;
            from.require_auth();

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
                env.storage()
                    .persistent()
                    .set(&TokenKey::Owner(token_id), &to);
                // Clear any existing approval on transfer.
                env.storage()
                    .persistent()
                    .remove(&TokenKey::Approval(token_id));

                bump_token(&env, &TokenKey::Owner(token_id));
                events::transferred(&env, &from, &to, token_id);
            }

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

            env.storage()
                .persistent()
                .set(&TokenKey::Owner(token_id), &to);
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

        /// Return the total number of minted (non-burned) tokens.
        #[must_use]
        pub fn total_supply(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0)
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
