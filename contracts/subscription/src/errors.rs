// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SubscriptionError {
    /// `initialize` was called on an already-initialized contract.
    AlreadyInitialized = 1,
    /// An operation was attempted before the contract was initialized.
    NotInitialized = 2,
    /// Caller is not authorized (e.g. non-provider calling `charge`).
    NotAuthorized = 3,
    /// Amount is zero or negative.
    InvalidAmount = 4,
    /// Interval is zero.
    InvalidInterval = 5,
    /// Subscriber already has an active subscription.
    AlreadySubscribed = 6,
    /// No subscription found for this subscriber.
    NotSubscribed = 7,
    /// Subscription has been cancelled.
    SubscriptionInactive = 8,
    /// The charge interval has not elapsed since the last payment.
    IntervalNotElapsed = 9,
    /// Subscriber has not granted sufficient token allowance to this contract.
    InsufficientAllowance = 10,
    /// Plan with this ID already exists.
    PlanAlreadyExists = 11,
    /// Plan does not exist.
    PlanNotFound = 12,
    /// Plan is not active and cannot be subscribed to.
    PlanInactive = 13,
    /// Prepay discount is outside 0..=10_000 basis points.
    InvalidDiscount = 14,
    /// A billing calculation overflowed.
    ArithmeticOverflow = 15,
}

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
