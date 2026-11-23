#![no_std]
#![deny(missing_docs)]
//! Timelock contract template.
//!
//! Locks tokens until a target ledger, after which the beneficiary can release
//! them; the depositor may cancel while still active.

use soroban_sdk::{Address, Env, Vec, contract, contractclient, contractimpl, token};

/// Number of tranches stored in persistent storage.
const TRANCHE_COUNT: u32 = 0;

/// Number of tokens tracked by a multi-token timelock instance.
const TOKEN_COUNT: u32 = 1;

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use errors::TimelockError;
pub use storage::{DataKey, ReleaseTranche, TimelockInfo, TimelockState};

/// Minimal cross-contract interface for the staking contract used to generate
/// yield on locked balances.
#[contractclient(name = "StakingContractClient")]
pub trait StakingContract {
    /// Stake `amount` tokens on behalf of `staker`.
    fn stake(env: Env, staker: Address, amount: i128);
    /// Begin unbonding `amount` tokens for `staker`.
    fn unstake(env: Env, staker: Address, amount: i128);
    /// Withdraw tokens that have finished unbonding for `staker`.
    fn withdraw(env: Env, staker: Address);
}

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn get_required<T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Result<T, TimelockError> {
    env.storage()
        .instance()
        .get(key)
        .ok_or(TimelockError::NotInitialized)
}

/// Timelock contract: holds tokens until a specified ledger, then releases to a beneficiary.
///
/// Lifecycle: `Active → Released` (via `release`) or `Active → Cancelled` (via `cancel`).
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;
    use storage::DataKey::*;

    #[contract]
    pub struct TimelockContract;

    #[contractimpl]
    impl TimelockContract {
        /// Initialize the timelock. Transfers `amount` tokens from `admin` to the contract.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::AlreadyInitialized`] if already set up.
        /// Returns [`TimelockError::InvalidAmount`] if `amount` <= 0.
        /// Returns [`TimelockError::InvalidReleaseLedger`] if `release_ledger` is not at least
        /// [`soroban_common::MIN_DEADLINE_BUFFER`] ledgers in the future.
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
            beneficiary: Address,
            release_ledger: u32,
            amount: i128,
        ) -> Result<(), TimelockError> {
            if env.storage().instance().has(&State) {
                return Err(TimelockError::AlreadyInitialized);
            }
            if amount <= 0 {
                return Err(TimelockError::InvalidAmount);
            }
            soroban_common::validate_deadline(&env, release_ledger)
                .map_err(|_| TimelockError::InvalidReleaseLedger)?;

            admin.require_auth();

            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );

            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Token, &token);
            env.storage().instance().set(&Beneficiary, &beneficiary);
            env.storage()
                .instance()
                .set(&ReleaseLedger, &release_ledger);
            env.storage().instance().set(&Amount, &amount);
            env.storage().instance().set(&State, &TimelockState::Active);

            // Map the legacy single-token configuration onto token-keyed storage
            // so that multi-token deposit/release entry points work uniformly.
            env.storage()
                .instance()
                .set(&LockedBalance(token.clone()), &amount);
            env.storage()
                .instance()
                .set(&TokenReleaseLedger(token.clone()), &release_ledger);
            env.storage().instance().set(&TokenCount, &1u32);
            env.storage().instance().set(&TokenAt(0), &token);

            bump_instance(&env);
            events::initialized(&env, &admin, &beneficiary, release_ledger, amount);

            Ok(())
        }

        /// Initialize the timelock and authorize a staking contract for yield generation.
        ///
        /// Transfers `amount` tokens from `admin` to the contract and stores the
        /// authorized staking contract plus the recipient of any accrued yield.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::AlreadyInitialized`] if already set up.
        /// Returns [`TimelockError::InvalidAmount`] if `amount` <= 0.
        /// Returns [`TimelockError::InvalidReleaseLedger`] if `release_ledger` <= current ledger.
        pub fn initialize_with_staking(
            env: Env,
            admin: Address,
            token: Address,
            beneficiary: Address,
            release_ledger: u32,
            amount: i128,
            staking_contract: Address,
            yield_recipient: Address,
        ) -> Result<(), TimelockError> {
            if env.storage().instance().has(&State) {
                return Err(TimelockError::AlreadyInitialized);
            }
            if amount <= 0 {
                return Err(TimelockError::InvalidAmount);
            }
            if release_ledger <= env.ledger().sequence() {
                return Err(TimelockError::InvalidReleaseLedger);
            }

            admin.require_auth();

            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );

            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Token, &token);
            env.storage().instance().set(&Beneficiary, &beneficiary);
            env.storage()
                .instance()
                .set(&ReleaseLedger, &release_ledger);
            env.storage().instance().set(&Amount, &amount);
            env.storage().instance().set(&State, &TimelockState::Active);
            env.storage()
                .instance()
                .set(&StakingContract, &staking_contract);
            env.storage()
                .instance()
                .set(&YieldRecipient, &yield_recipient);
            env.storage().instance().set(&Deposited, &false);

            // Map the legacy single-token configuration onto token-keyed storage.
            env.storage()
                .instance()
                .set(&LockedBalance(token.clone()), &amount);
            env.storage()
                .instance()
                .set(&TokenReleaseLedger(token.clone()), &release_ledger);
            env.storage().instance().set(&TokenCount, &1u32);
            env.storage().instance().set(&TokenAt(0), &token);

            bump_instance(&env);
            events::initialized(&env, &admin, &beneficiary, release_ledger, amount);

            Ok(())
        }

        /// Deposit the locked token balance into the authorized staking contract.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::AlreadyReleased`] / [`TimelockError::AlreadyCancelled`]
        /// if the timelock is no longer active.
        /// Returns [`TimelockError::StakingNotConfigured`] if no staking contract is set.
        /// Returns [`TimelockError::AlreadyDeposited`] if the balance is already staked.
        pub fn deposit_locked_balance(env: Env) -> Result<(), TimelockError> {
            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            let staking_contract: Address = env
                .storage()
                .instance()
                .get(&StakingContract)
                .ok_or(TimelockError::StakingNotConfigured)?;

            let deposited: bool = env
                .storage()
                .instance()
                .get(&Deposited)
                .unwrap_or(false);
            if deposited {
                return Err(TimelockError::AlreadyDeposited);
            }

            let token: Address = get_required(&env, &Token)?;
            let amount: i128 = get_required(&env, &Amount)?;

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &staking_contract,
                &amount,
            );

            StakingContractClient::new(&env, &staking_contract).stake(
                &env.current_contract_address(),
                &amount,
            );

            env.storage().instance().set(&Deposited, &true);
            env.storage().instance().set(&StakedAmount, &amount);
            bump_instance(&env);

            events::deposited(&env, &staking_contract, amount);

            Ok(())
        }

        /// Unstake and withdraw the deposited balance from the staking contract.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::AlreadyReleased`] / [`TimelockError::AlreadyCancelled`]
        /// if the timelock is no longer active.
        /// Returns [`TimelockError::StakingNotConfigured`] if no staking contract is set.
        /// Returns [`TimelockError::NotDeposited`] if no balance is currently staked.
        pub fn withdraw_and_unbond(env: Env) -> Result<(), TimelockError> {
            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            let staking_contract: Address = env
                .storage()
                .instance()
                .get(&StakingContract)
                .ok_or(TimelockError::StakingNotConfigured)?;

            let deposited: bool = env
                .storage()
                .instance()
                .get(&Deposited)
                .unwrap_or(false);
            if !deposited {
                return Err(TimelockError::NotDeposited);
            }

            let staked_amount: i128 = get_required(&env, &StakedAmount)?;

            let client = StakingContractClient::new(&env, &staking_contract);
            client.unstake(&env.current_contract_address(), &staked_amount);
            client.withdraw(&env.current_contract_address());

            env.storage().instance().set(&Deposited, &false);
            bump_instance(&env);

            events::withdrawn(&env, &staking_contract, staked_amount);

            Ok(())
        }

        /// Initialize the timelock with multiple release tranches.
        /// Transfers total tokens from `admin` to the contract.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::AlreadyInitialized`] if already set up.
        /// Returns [`TimelockError::InvalidAmount`] if any tranche amount <= 0.
        /// Returns [`TimelockError::InvalidReleaseLedger`] if first tranche ledger <= current ledger.
        pub fn initialize_with_tranches(
            env: Env,
            admin: Address,
            token: Address,
            beneficiary: Address,
            tranches: Vec<ReleaseTranche>,
        ) -> Result<(), TimelockError> {
            if env.storage().instance().has(&State) {
                return Err(TimelockError::AlreadyInitialized);
            }
            if tranches.is_empty() {
                return Err(TimelockError::InvalidAmount);
            }

            admin.require_auth();

            // Validate tranches and calculate total amount.
            //
            // Each tranche must have a positive amount and a release ledger
            // strictly greater than the previous one (monotonically increasing).
            // The first tranche must also be in the future relative to the
            // current ledger; because the sequence is monotonic, this
            // guarantees every subsequent tranche is also in the future
            // (issue #1200 — previously only the *last* tranche was checked,
            // allowing earlier tranches with already-past release ledgers).
            let mut total_amount: i128 = 0;
            let mut prev_ledger: u32 = 0;
            for tranche in tranches.iter() {
                if tranche.amount <= 0 {
                    return Err(TimelockError::InvalidAmount);
                }
                if tranche.release_ledger <= prev_ledger {
                    return Err(TimelockError::InvalidReleaseLedger);
                }
                total_amount = total_amount.checked_add(tranche.amount)
                    .ok_or(TimelockError::InvalidAmount)?;
                prev_ledger = tranche.release_ledger;
            }

            let first_release_ledger = tranches.get(0).unwrap().release_ledger;
            if first_release_ledger <= env.ledger().sequence() {
                return Err(TimelockError::InvalidReleaseLedger);
            }

            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &total_amount,
            );

            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Token, &token);
            env.storage().instance().set(&Beneficiary, &beneficiary);

            // Store each tranche and its release flag in persistent storage.
            let released_count = tranches.len();
            for i in 0..released_count {
                let tranche = tranches.get(i).unwrap();
                let tranche_key = DataKey::Tranche(i);
                env.storage().persistent().set(&tranche_key, &tranche);
                env.storage().persistent().extend_ttl(
                    &tranche_key,
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );

                let released_key = DataKey::TrancheReleased(i);
                env.storage().persistent().set(&released_key, &false);
                env.storage().persistent().extend_ttl(
                    &released_key,
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
            }
            env.storage().instance().set(&TRANCHE_COUNT, &released_count);

            // Also set legacy fields for backwards compatibility
            let first_tranche = tranches.get(0).unwrap();
            env.storage().instance().set(&ReleaseLedger, &first_tranche.release_ledger);
            env.storage().instance().set(&Amount, &total_amount);

            // Map the legacy single-token configuration onto token-keyed storage.
            env.storage()
                .instance()
                .set(&LockedBalance(token.clone()), &total_amount);
            env.storage()
                .instance()
                .set(&TokenReleaseLedger(token.clone()), &first_tranche.release_ledger);
            env.storage().instance().set(&TokenCount, &1u32);
            env.storage().instance().set(&TokenAt(0), &token);

            env.storage().instance().set(&State, &TimelockState::Active);

            bump_instance(&env);
            events::initialized(&env, &admin, &beneficiary, first_tranche.release_ledger, total_amount);

            Ok(())
        }

        /// Deposit `amount` of `token` into the timelock, unlocking at `release_ledger`.
        ///
        /// Multiple distinct tokens may be locked in a single contract instance;
        /// balances and schedules are keyed by token address.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::AlreadyReleased`] / [`TimelockError::AlreadyCancelled`]
        /// if the timelock is no longer active.
        /// Returns [`TimelockError::InvalidAmount`] if `amount` <= 0.
        /// Returns [`TimelockError::InvalidReleaseLedger`] if `release_ledger` <= current ledger.
        pub fn deposit(
            env: Env,
            token: Address,
            amount: i128,
            release_ledger: u32,
        ) -> Result<(), TimelockError> {
            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            if amount <= 0 {
                return Err(TimelockError::InvalidAmount);
            }
            if release_ledger <= env.ledger().sequence() {
                return Err(TimelockError::InvalidReleaseLedger);
            }

            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();

            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );

            // Accumulate the locked balance for this token.
            let existing: i128 = env
                .storage()
                .instance()
                .get(&LockedBalance(token.clone()))
                .unwrap_or(0);
            let new_balance = existing
                .checked_add(amount)
                .ok_or(TimelockError::InvalidAmount)?;
            env.storage()
                .instance()
                .set(&LockedBalance(token.clone()), &new_balance);

            // Track the token in the instance's token registry if it is new.
            let count: u32 = env.storage().instance().get(&TokenCount).unwrap_or(0);
            let mut known = false;
            for i in 0..count {
                if let Some(existing_token) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&TokenAt(i))
                {
                    if existing_token == token {
                        known = true;
                        break;
                    }
                }
            }
            if !known {
                env.storage().instance().set(&TokenAt(count), &token);
                env.storage().instance().set(&TokenCount, &(count + 1));
            }

            // Record the schedule for this token, keeping the earliest ledger.
            let prev_ledger: u32 = env
                .storage()
                .instance()
                .get(&TokenReleaseLedger(token.clone()))
                .unwrap_or(u32::MAX);
            if release_ledger < prev_ledger {
                env.storage()
                    .instance()
                    .set(&TokenReleaseLedger(token.clone()), &release_ledger);
            }

            bump_instance(&env);
            events::deposited_token(&env, &token, amount, release_ledger);

            Ok(())
        }

        /// Release all matured locked tokens of `token` to the beneficiary.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::AlreadyReleased`] if tokens were already released.
        /// Returns [`TimelockError::AlreadyCancelled`] if the timelock was cancelled.
        /// Returns [`TimelockError::NotYetReleasable`] if no balance has matured yet.
        pub fn release(env: Env, token: Address) -> Result<(), TimelockError> {
            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            let beneficiary: Address = get_required(&env, &Beneficiary)?;

            // If tokens were deposited into staking, withdraw them first so the
            // principal and any accrued yield are available for distribution.
            let staking_contract: Option<Address> =
                env.storage().instance().get(&StakingContract);
            let deposited: bool = env
                .storage()
                .instance()
                .get(&Deposited)
                .unwrap_or(false);

            if let Some(staking) = staking_contract.clone() {
                if deposited {
                    let staked_amount: i128 = get_required(&env, &StakedAmount)?;
                    let client = StakingContractClient::new(&env, &staking);
                    client.unstake(&env.current_contract_address(), &staked_amount);
                    client.withdraw(&env.current_contract_address());
                    env.storage().instance().set(&Deposited, &false);
                }
            }

            // Check if this is a multi-tranche timelock
            let tranche_count: Option<u32> = env.storage().instance().get(&TRANCHE_COUNT);

            if let Some(count) = tranche_count {
                // Multi-tranche release
                let current_ledger = env.ledger().sequence();
                let mut total_released: i128 = 0;
                let mut any_new_released = false;
                let mut all_released = true;

                for i in 0..count {
                    let tranche_key = DataKey::Tranche(i);
                    let tranche: ReleaseTranche = env
                        .storage()
                        .persistent()
                        .get(&tranche_key)
                        .ok_or(TimelockError::NotInitialized)?;
                    env.storage().persistent().extend_ttl(
                        &tranche_key,
                        LEDGER_LIFETIME_THRESHOLD,
                        LEDGER_BUMP_AMOUNT,
                    );

                    let released_key = DataKey::TrancheReleased(i);
                    let is_released: bool = env
                        .storage()
                        .persistent()
                        .get(&released_key)
                        .unwrap_or(false);
                    env.storage().persistent().extend_ttl(
                        &released_key,
                        LEDGER_LIFETIME_THRESHOLD,
                        LEDGER_BUMP_AMOUNT,
                    );

                    if !is_released && current_ledger >= tranche.release_ledger {
                        env.storage().persistent().set(&released_key, &true);
                        env.storage().persistent().extend_ttl(
                            &released_key,
                            LEDGER_LIFETIME_THRESHOLD,
                            LEDGER_BUMP_AMOUNT,
                        );
                        total_released += tranche.amount;
                        any_new_released = true;
                    } else if !is_released {
                        all_released = false;
                    }
                }

                if !any_new_released {
                    return Err(TimelockError::NotYetReleasable);
                }

                if all_released {
                    env.storage().instance().set(&State, &TimelockState::Released);
                }
                bump_instance(&env);

                token::Client::new(&env, &token).transfer(
                    &env.current_contract_address(),
                    &beneficiary,
                    &total_released,
                );

                events::released(&env, &beneficiary, total_released);
            } else {
                // Legacy single-tranche release
                let release_ledger: u32 = get_required(&env, &ReleaseLedger)?;
                if env.ledger().sequence() < release_ledger {
                    return Err(TimelockError::NotYetReleasable);
                }

                let amount: i128 = get_required(&env, &Amount)?;

                env.storage()
                    .instance()
                    .set(&State, &TimelockState::Released);
                bump_instance(&env);

                // Distribute principal to the beneficiary and any accrued yield
                // to the designated yield recipient when staking is configured.
                if let Some(staking) = staking_contract {
                    let yield_recipient: Address = env
                        .storage()
                        .instance()
                        .get(&YieldRecipient)
                        .ok_or(TimelockError::StakingNotConfigured)?;

                    let balance = token::Client::new(&env, &token)
                        .balance(&env.current_contract_address());
                    let yield_amount = balance.saturating_sub(amount);

                    token::Client::new(&env, &token).transfer(
                        &env.current_contract_address(),
                        &beneficiary,
                        &amount,
                    );

                    if yield_amount > 0 {
                        token::Client::new(&env, &token).transfer(
                            &env.current_contract_address(),
                            &yield_recipient,
                            &yield_amount,
                        );
                    }

                    events::yield_distributed(
                        &env,
                        &staking,
                        &yield_recipient,
                        amount,
                        yield_amount,
                    );
                } else {
                    token::Client::new(&env, &token).transfer(
                        &env.current_contract_address(),
                        &beneficiary,
                        &amount,
                    );
                }

                events::released(&env, &beneficiary, amount);
            }

            Ok(())
        }

        /// Release a specific `amount` of the currently matured balance of `token`.
        ///
        /// Unlike [`Self::release`], this leaves any remaining matured balance
        /// available for subsequent partial claims.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::AlreadyReleased`] / [`TimelockError::AlreadyCancelled`]
        /// if the timelock is no longer active.
        /// Returns [`TimelockError::InvalidAmount`] if `amount` <= 0.
        /// Returns [`TimelockError::NotYetReleasable`] if `amount` exceeds the matured balance.
        pub fn release_partial(
            env: Env,
            token: Address,
            amount: i128,
        ) -> Result<(), TimelockError> {
            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            if amount <= 0 {
                return Err(TimelockError::InvalidAmount);
            }

            let beneficiary: Address = get_required(&env, &Beneficiary)?;

            // Compute the currently matured claimable balance for this token.
            let claimable = Self::claimable_amount_internal(&env, &token)?;
            if amount > claimable {
                return Err(TimelockError::NotYetReleasable);
            }

            // Deduct the requested amount from the locked balance.
            let locked: i128 = env
                .storage()
                .instance()
                .get(&LockedBalance(token.clone()))
                .unwrap_or(0);
            let remaining = locked.saturating_sub(amount);
            env.storage()
                .instance()
                .set(&LockedBalance(token.clone()), &remaining);

            bump_instance(&env);

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &beneficiary,
                &amount,
            );

            events::partial_released(&env, &token, &beneficiary, amount);

            Ok(())
        }

        /// Return the currently matured claimable balance for `token`.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        pub fn claimable_amount(env: Env, token: Address) -> Result<i128, TimelockError> {
            Self::claimable_amount_internal(&env, &token)
        }

        /// Internal helper computing the matured claimable balance for `token`.
        fn claimable_amount_internal(
            env: &Env,
            token: &Address,
        ) -> Result<i128, TimelockError> {
            let current_ledger = env.ledger().sequence();

            // Multi-tranche timelocks track per-tranche maturity.
            if let Some(count) = env.storage().instance().get::<_, u32>(&TRANCHE_COUNT) {
                let mut claimable: i128 = 0;
                for i in 0..count {
                    let tranche_key = DataKey::Tranche(i);
                    let tranche: ReleaseTranche = env
                        .storage()
                        .persistent()
                        .get(&tranche_key)
                        .ok_or(TimelockError::NotInitialized)?;
                    env.storage().persistent().extend_ttl(
                        &tranche_key,
                        LEDGER_LIFETIME_THRESHOLD,
                        LEDGER_BUMP_AMOUNT,
                    );

                    let released_key = DataKey::TrancheReleased(i);
                    let is_released: bool = env
                        .storage()
                        .persistent()
                        .get(&released_key)
                        .unwrap_or(false);
                    env.storage().persistent().extend_ttl(
                        &released_key,
                        LEDGER_LIFETIME_THRESHOLD,
                        LEDGER_BUMP_AMOUNT,
                    );

                    if !is_released && current_ledger >= tranche.release_ledger {
                        claimable = claimable.saturating_add(tranche.amount);
                    }
                }
                return Ok(claimable);
            }

            // Legacy single-tranche timelock.
            let release_ledger: u32 = env
                .storage()
                .instance()
                .get(&TokenReleaseLedger(token.clone()))
                .or_else(|| env.storage().instance().get(&ReleaseLedger))
                .unwrap_or(0);
            if current_ledger < release_ledger {
                return Ok(0);
            }

            Ok(env
                .storage()
                .instance()
                .get(&LockedBalance(token.clone()))
                .unwrap_or(0))
        }

        /// Cancel the timelock and return tokens to admin. Admin only; works while in `Active` state.
        ///
        /// For multi-tranche timelocks, only the unreleased balance is returned:
        /// tranches already released to the beneficiary via `release()` are
        /// subtracted from the total locked amount so the contract never
        /// attempts to transfer more than it holds (issue #1199).
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::NotAuthorized`] if caller is not the admin.
        /// Returns [`TimelockError::AlreadyReleased`] if tokens were already released.
        /// Returns [`TimelockError::AlreadyCancelled`] if already cancelled.
        pub fn cancel(env: Env) -> Result<(), TimelockError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();

            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            let token: Address = get_required(&env, &Token)?;
            let amount: i128 = get_required(&env, &Amount)?;

            // For multi-tranche timelocks, subtract the amounts already released
            // to the beneficiary so we only return what the contract still holds
            // (issue #1199 — insolvency bug).
            let tranche_count: Option<u32> = env.storage().instance().get(&TRANCHE_COUNT);
            let refund_amount = if let Some(count) = tranche_count {
                let mut already_released: i128 = 0;
                for i in 0..count {
                    let released_key = DataKey::TrancheReleased(i);
                    let is_released: bool = env
                        .storage()
                        .persistent()
                        .get(&released_key)
                        .unwrap_or(false);
                    if is_released {
                        let tranche_key = DataKey::Tranche(i);
                        let tranche: ReleaseTranche = env
                            .storage()
                            .persistent()
                            .get(&tranche_key)
                            .ok_or(TimelockError::NotInitialized)?;
                        already_released = already_released
                            .checked_add(tranche.amount)
                            .ok_or(TimelockError::InvalidAmount)?;
                    }
                }
                amount
                    .checked_sub(already_released)
                    .ok_or(TimelockError::InvalidAmount)?
            } else {
                amount
            };

            env.storage()
                .instance()
                .set(&State, &TimelockState::Cancelled);
            bump_instance(&env);

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &admin,
                &refund_amount,
            );

            events::cancelled(&env, &admin, refund_amount);

            Ok(())
        }

        /// Reschedule an unreleased tranche to a new release ledger.
        ///
        /// Requires mutual authorization from both the admin and the beneficiary.
        ///
        /// # Errors
        ///
        /// Returns [`TimelockError::NotInitialized`] if not yet set up.
        /// Returns [`TimelockError::NotAuthorized`] if either party fails authorization.
        /// Returns [`TimelockError::TrancheAlreadyReleased`] if the tranche was already released.
        /// Returns [`TimelockError::InvalidReleaseLedger`] if `new_release_ledger` is not at least
        /// [`soroban_common::MIN_DEADLINE_BUFFER`] ledgers in the future.
        pub fn reschedule_tranche(
            env: Env,
            tranche_id: u32,
            new_release_ledger: u32,
        ) -> Result<(), TimelockError> {
            let admin: Address = get_required(&env, &Admin)?;
            let beneficiary: Address = get_required(&env, &Beneficiary)?;

            // Mutual authorization: both admin and beneficiary must approve.
            admin.require_auth();
            beneficiary.require_auth();

            let mut tranches: Vec<ReleaseTranche> = get_required(&env, &Tranches)?;
            let mut released: Vec<bool> = get_required(&env, &ReleasedTranches)?;

            if tranche_id >= tranches.len() {
                return Err(TimelockError::InvalidAmount);
            }

            if released.get(tranche_id).unwrap() {
                return Err(TimelockError::TrancheAlreadyReleased);
            }

            soroban_common::validate_deadline(&env, new_release_ledger)
                .map_err(|_| TimelockError::InvalidReleaseLedger)?;

            let old_ledger = tranches.get(tranche_id).unwrap().release_ledger;
            let mut tranche = tranches.get(tranche_id).unwrap();
            tranche.release_ledger = new_release_ledger;
            tranches.set(tranche_id, tranche);

            env.storage().instance().set(&Tranches, &tranches);
            env.storage().instance().set(&ReleasedTranches, &released);
            bump_instance(&env);

            events::tranche_rescheduled(&env, tranche_id, old_ledger, new_release_ledger);

            Ok(())
        }

        /// Admin-only: reassign the beneficiary while the timelock is active.
        ///
        /// # Errors
        /// - [`TimelockError::NotInitialized`] if the contract has not been initialized.
        /// - [`TimelockError::NotAuthorized`] if the caller is not the admin.
        /// - [`TimelockError::AlreadyReleased`] if the timelock has been released.
        /// - [`TimelockError::AlreadyCancelled`] if the timelock has been cancelled.
        pub fn reassign_beneficiary(
            env: Env,
            new_beneficiary: Address,
        ) -> Result<(), TimelockError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();

            let state: TimelockState = get_required(&env, &State)?;
            match state {
                TimelockState::Released => return Err(TimelockError::AlreadyReleased),
                TimelockState::Cancelled => return Err(TimelockError::AlreadyCancelled),
                TimelockState::Active => {}
            }

            let old_beneficiary: Address = get_required(&env, &Beneficiary)?;
            env.storage().instance().set(&Beneficiary, &new_beneficiary);
            bump_instance(&env);

            events::beneficiary_reassigned(&env, &admin, &old_beneficiary, &new_beneficiary);

            Ok(())
        }

        /// Return full timelock details.
        #[must_use]
        pub fn get_info(env: Env) -> Result<TimelockInfo, TimelockError> {
            let mut tranches: Vec<ReleaseTranche> = Vec::new(&env);
            if let Some(count) = env.storage().instance().get::<_, u32>(&TRANCHE_COUNT) {
                for i in 0..count {
                    let tranche_key = DataKey::Tranche(i);
                    if let Some(tranche) = env.storage().persistent().get(&tranche_key) {
                        env.storage().persistent().extend_ttl(
                            &tranche_key,
                            LEDGER_LIFETIME_THRESHOLD,
                            LEDGER_BUMP_AMOUNT,
                        );
                        tranches.push_back(tranche);
                    }
                }
            }

            Ok(TimelockInfo {
                admin: get_required(&env, &Admin)?,
                token: get_required(&env, &Token)?,
                beneficiary: get_required(&env, &Beneficiary)?,
                release_ledger: get_required(&env, &ReleaseLedger)?,
                amount: get_required(&env, &Amount)?,
                state: get_required(&env, &State)?,
                tranches,
                staking_contract: env.storage().instance().get(&StakingContract),
                yield_recipient: env.storage().instance().get(&YieldRecipient),
                deposited: env
                    .storage()
                    .instance()
                    .get(&Deposited)
                    .unwrap_or(false),
                staked_amount: env
                    .storage()
                    .instance()
                    .get(&StakedAmount)
                    .unwrap_or(0),
            })
        }

        /// Return `true` if at least one tranche is releasable and the state is still `Active`.
        #[must_use]
        pub fn is_releasable(env: Env) -> bool {
            let state: Option<TimelockState> = env.storage().instance().get(&State);
            if !matches!(state, Some(TimelockState::Active)) {
                return false;
            }

            let current_ledger = env.ledger().sequence();

            // Check for multi-tranche timelock
            if let Some(count) = env.storage().instance().get::<_, u32>(&TRANCHE_COUNT) {
                for i in 0..count {
                    let tranche_key = DataKey::Tranche(i);
                    if let Some(tranche) = env
                        .storage()
                        .persistent()
                        .get::<_, ReleaseTranche>(&tranche_key)
                    {
                        env.storage().persistent().extend_ttl(
                            &tranche_key,
                            LEDGER_LIFETIME_THRESHOLD,
                            LEDGER_BUMP_AMOUNT,
                        );
                        if current_ledger >= tranche.release_ledger {
                            return true;
                        }
                    }
                }
                return false;
            }

            // Legacy single-tranche check
            let release_ledger: u32 = env.storage().instance().get(&ReleaseLedger).unwrap_or(0);
            current_ledger >= release_ledger
        }

        /// Return ledgers remaining until release (negative if already past).
        pub fn get_remaining_ledgers(env: Env) -> i64 {
            let release_ledger: u32 = env.storage().instance().get(&ReleaseLedger).unwrap_or(0);
            release_ledger as i64 - env.ledger().sequence() as i64
        }

        /// Return the locked balance currently held for `token`.
        #[must_use]
        pub fn locked_balance(env: Env, token: Address) -> i128 {
            env.storage()
                .instance()
                .get(&LockedBalance(token))
                .unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod prop_test;
