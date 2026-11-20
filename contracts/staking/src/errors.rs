// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StakingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    NoStake = 5,
    InsufficientStake = 6,
    NoRewards = 7,
    CompoundTokenMismatch = 8,
    UnbondingNotComplete = 9,
    NoUnbondRequest = 10,
    UnbondRequestPending = 11,
    InvalidTranche = 12,
    InvalidPenalty = 13,
    InvalidRewardRate = 14,
    InvalidPeriodEnd = 15,
    InsufficientRewardReserves = 16,
    StillLocked = 17,
    InvalidLockup = 18,
    PermitExpired = 19,
    Overflow = 20,
    InvalidSlashPercentage = 21,
    UnauthorizedSlasher = 22,
    NoPendingUnbond = 23,
    InvalidSaturationConfig = 24,
    ContractNotPaused = 25,
}

impl_display_error!(
    StakingError,
    AlreadyInitialized     => "already initialized",
    NotInitialized         => "not initialized",
    Unauthorized           => "not authorized",
    InvalidAmount          => "invalid amount",
    NoStake                => "no stake",
    InsufficientStake      => "insufficient stake",
    NoRewards              => "no rewards",
    CompoundTokenMismatch  => "compound token mismatch",
    UnbondingNotComplete   => "unbonding not complete",
    NoUnbondRequest        => "no unbond request",
    UnbondRequestPending   => "unbond request pending",
    InvalidTranche         => "invalid unbond tranche",
    InvalidPenalty         => "invalid penalty",
    InvalidRewardRate      => "invalid reward rate",
    InvalidPeriodEnd       => "invalid period end",
    InsufficientRewardReserves => "insufficient reward reserves",
    StillLocked            => "stake is locked",
    InvalidLockup          => "invalid lockup",
    PermitExpired          => "permit expired",
    Overflow               => "overflow",
    InvalidSlashPercentage => "invalid slash percentage",
    UnauthorizedSlasher    => "unauthorized slasher",
    NoPendingUnbond        => "no pending unbond request",
    InvalidSaturationConfig => "invalid saturation config",
    ContractNotPaused      => "contract not paused",
);
