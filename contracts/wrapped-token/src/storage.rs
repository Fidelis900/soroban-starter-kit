// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    Admin,
    WrappedToken,
    UnderlyingToken,
    TotalWrapped,
    /// Instance storage – whether the contract is paused (`bool`). Only used
    /// when the `pausable` feature is enabled.
    Paused,
    /// Instance storage – optional cap on the cumulative amount any single
    /// address may ever wrap (`i128`). Absent means uncapped.
    MaxWrapPerAddress,
    /// Persistent storage – cumulative amount wrapped so far by a given
    /// `Address` (`i128`). Only tracked when `MaxWrapPerAddress` is set.
    WrappedByAddress(Address),
    /// Instance storage – whether `wrap` is paused (`bool`). Only used when
    /// the `pausable` feature is enabled.
    WrapPaused,
    /// Instance storage – whether `unwrap` is paused (`bool`). Only used when
    /// the `pausable` feature is enabled.
    UnwrapPaused,
    /// Instance storage – protocol fee charged on `wrap`, in basis points (`u32`).
    /// Absent means no fee.
    WrapFeeBps,
    /// Instance storage – protocol fee charged on `unwrap`, in basis points (`u32`).
    /// Absent means no fee.
    UnwrapFeeBps,
    /// Instance storage – address receiving collected protocol fees (`Address`).
    FeeRecipient,
    /// Instance storage – optional cap on the aggregate amount unwrapped
    /// inside one sliding ledger window (`i128`). Absent means unlimited.
    UnwrapRateLimit,
    /// Instance storage – length of the unwrap rate-limit window in ledgers (`u32`).
    UnwrapRateWindow,
    /// Instance storage – `(window_start_ledger, amount_unwrapped)` for the
    /// current unwrap rate-limit window.
    UnwrapRateUsage,
    /// Instance storage – when `true`, the unwrap rate limit is paused and
    /// unwraps are not throttled (admin migration bypass).
    UnwrapRateLimitPaused,
    /// Instance storage – optional proof-of-reserve oracle contract (`Address`).
    /// Absent means wrapping is not checked against an external reserve.
    PorOracle,
    /// Instance storage – optional minimum amount (`i128`) accepted by `wrap`
    /// and `unwrap`. Absent means no minimum is enforced.
    MinAmount,
    /// Instance storage – reentrancy lock (`bool`) held for the duration of
    /// `wrap`, `unwrap` and their batch variants (#1435). Absent means unlocked.
    ReentrancyLock,
}

/// Proof-of-reserve snapshot returned by
/// [`WrappedTokenContract::get_reserve_status`](crate::WrappedTokenContract::get_reserve_status) (#1436).
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReserveStatus {
    /// Wrapped-token supply tracked by the contract.
    pub total_wrapped: i128,
    /// Underlying collateral actually held by the contract.
    pub total_locked: i128,
    /// `true` when `total_wrapped <= total_locked`, i.e. the 1:1 backing holds.
    pub is_fully_backed: bool,
}

#[cfg(test)]
mod discriminant_tests {
    use super::*;
    use soroban_sdk::{Env, testutils::Address as _};

    // In Soroban, #[contracttype] enums use the variant NAME as the XDR storage discriminant.
    // NEVER rename, reorder, or remove variants — doing so will corrupt on-chain storage for
    // any live deployment. To add a new key, append it at the END of the enum definition.
    //
    // This exhaustive match is the primary guard: it causes a COMPILE ERROR if a variant is
    // renamed or removed, and a non-exhaustive warning if one is added without updating here.
    fn wrapped_token_data_key_index(key: &DataKey) -> u32 {
        match key {
            DataKey::Admin => 0,
            DataKey::WrappedToken => 1,
            DataKey::UnderlyingToken => 2,
            DataKey::TotalWrapped => 3,
            DataKey::Paused => 4,
            DataKey::MaxWrapPerAddress => 5,
            DataKey::WrappedByAddress(_) => 6,
            DataKey::WrapPaused => 7,
            DataKey::UnwrapPaused => 8,
            DataKey::WrapFeeBps => 9,
            DataKey::UnwrapFeeBps => 10,
            DataKey::FeeRecipient => 11,
            DataKey::UnwrapRateLimit => 12,
            DataKey::UnwrapRateWindow => 13,
            DataKey::UnwrapRateUsage => 14,
            DataKey::UnwrapRateLimitPaused => 15,
            DataKey::PorOracle => 16,
            DataKey::MinAmount => 17,
            DataKey::ReentrancyLock => 18,
        }
    }

    #[test]
    fn data_key_discriminants_are_stable() {
        let env = Env::default();
        let addr = Address::generate(&env);

        assert_eq!(wrapped_token_data_key_index(&DataKey::Admin), 0);
        assert_eq!(wrapped_token_data_key_index(&DataKey::WrappedToken), 1);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnderlyingToken), 2);
        assert_eq!(wrapped_token_data_key_index(&DataKey::TotalWrapped), 3);
        assert_eq!(wrapped_token_data_key_index(&DataKey::Paused), 4);
        assert_eq!(wrapped_token_data_key_index(&DataKey::MaxWrapPerAddress), 5);
        assert_eq!(wrapped_token_data_key_index(&DataKey::WrappedByAddress(addr)), 6);
        assert_eq!(wrapped_token_data_key_index(&DataKey::WrapPaused), 7);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapPaused), 8);
        assert_eq!(wrapped_token_data_key_index(&DataKey::WrapFeeBps), 9);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapFeeBps), 10);
        assert_eq!(wrapped_token_data_key_index(&DataKey::FeeRecipient), 11);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapRateLimit), 12);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapRateWindow), 13);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapRateUsage), 14);
        assert_eq!(wrapped_token_data_key_index(&DataKey::UnwrapRateLimitPaused), 15);
        assert_eq!(wrapped_token_data_key_index(&DataKey::PorOracle), 16);
        assert_eq!(wrapped_token_data_key_index(&DataKey::MinAmount), 17);
        assert_eq!(wrapped_token_data_key_index(&DataKey::ReentrancyLock), 18);
    }
}
