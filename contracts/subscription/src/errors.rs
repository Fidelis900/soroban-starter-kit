// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SubscriptionError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    InvalidAmount = 4,
    InvalidInterval = 5,
    AlreadySubscribed = 6,
    NotSubscribed = 7,
    SubscriptionInactive = 8,
    IntervalNotElapsed = 9,
    InsufficientAllowance = 10,
    PlanAlreadyExists = 11,
    PlanNotFound = 12,
    PlanInactive = 13,
}

impl_display_error!(
    SubscriptionError,
    AlreadyInitialized     => "already initialized",
    NotInitialized         => "not initialized",
    NotAuthorized          => "not authorized",
    InvalidAmount          => "invalid amount",
    InvalidInterval        => "invalid interval",
    AlreadySubscribed      => "already subscribed",
    NotSubscribed          => "not subscribed",
    SubscriptionInactive   => "subscription inactive",
    IntervalNotElapsed     => "interval not elapsed",
    InsufficientAllowance  => "insufficient allowance",
    PlanAlreadyExists      => "plan already exists",
    PlanNotFound           => "plan not found",
    PlanInactive           => "plan inactive",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::SubscriptionError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
SubscriptionError::AlreadyInitialized = {}\n\
SubscriptionError::NotInitialized = {}\n\
SubscriptionError::NotAuthorized = {}\n\
SubscriptionError::InvalidAmount = {}\n\
SubscriptionError::InvalidInterval = {}\n\
SubscriptionError::AlreadySubscribed = {}\n\
SubscriptionError::NotSubscribed = {}\n\
SubscriptionError::SubscriptionInactive = {}\n\
SubscriptionError::IntervalNotElapsed = {}\n\
SubscriptionError::InsufficientAllowance = {}\n\
SubscriptionError::PlanAlreadyExists = {}\n\
SubscriptionError::PlanNotFound = {}\n\
SubscriptionError::PlanInactive = {}\n",
            SubscriptionError::AlreadyInitialized as u32,
            SubscriptionError::NotInitialized as u32,
            SubscriptionError::NotAuthorized as u32,
            SubscriptionError::InvalidAmount as u32,
            SubscriptionError::InvalidInterval as u32,
            SubscriptionError::AlreadySubscribed as u32,
            SubscriptionError::NotSubscribed as u32,
            SubscriptionError::SubscriptionInactive as u32,
            SubscriptionError::IntervalNotElapsed as u32,
            SubscriptionError::InsufficientAllowance as u32,
            SubscriptionError::PlanAlreadyExists as u32,
            SubscriptionError::PlanNotFound as u32,
            SubscriptionError::PlanInactive as u32,
        )
    }

    #[test]
    fn subscription_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
