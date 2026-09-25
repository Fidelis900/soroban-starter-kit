use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AirdropError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ClaimWindowClosed = 4,
    ClaimWindowNotClosed = 5,
    NothingToSweep = 6,
    InvalidDeadline = 7,
    InvalidAmount = 8,
    AlreadyClaimed = 9,
    InvalidProof = 10,
    RootNotSet = 11,
    RoundActive = 12,
    InvalidSignature = 13,
}
