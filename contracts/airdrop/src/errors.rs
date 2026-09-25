use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AirdropError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    /// Merkle root has not been set yet.
    RootNotSet = 4,
    /// The provided merkle proof is invalid.
    InvalidProof = 5,
    /// This address has already claimed their airdrop.
    AlreadyClaimed = 6,
    /// Claim amount is zero.
    InvalidAmount = 7,
    /// The claim deadline has passed; no further claims are accepted.
    ClaimWindowClosed = 8,
    /// The contract does not hold enough unlocked balance of the token to pay the claim.
    InsufficientBalance = 9,
    /// Vesting parameters are out of range.
    InvalidVestingConfig = 10,
    /// No vesting schedule exists for this recipient and token.
    NoVestingSchedule = 11,
    /// No vested tokens are currently releasable.
    NothingToRelease = 12,
    /// A batch contains the same `(recipient, token)` pair more than once.
    DuplicateEntry = 13,
    /// An arithmetic operation overflowed.
    ArithmeticOverflow = 14,
    /// `claim_batch` was called with no entries.
    EmptyBatch = 15,
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::AirdropError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
AirdropError::AlreadyInitialized = {}\n\
AirdropError::NotInitialized = {}\n\
AirdropError::Unauthorized = {}\n\
AirdropError::RootNotSet = {}\n\
AirdropError::InvalidProof = {}\n\
AirdropError::AlreadyClaimed = {}\n\
AirdropError::InvalidAmount = {}\n\
AirdropError::ClaimWindowClosed = {}\n",
            AirdropError::AlreadyInitialized as u32,
            AirdropError::NotInitialized as u32,
            AirdropError::Unauthorized as u32,
            AirdropError::RootNotSet as u32,
            AirdropError::InvalidProof as u32,
            AirdropError::AlreadyClaimed as u32,
            AirdropError::InvalidAmount as u32,
            AirdropError::ClaimWindowClosed as u32,
        )
    }

    #[test]
    fn airdrop_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
    ClaimWindowClosed = 4,
    ClaimWindowNotClosed = 5,
    NothingToSweep = 6,
    InvalidDeadline = 7,
    InvalidAmount = 8,
    AlreadyClaimed = 9,
    InvalidProof = 10,
}
