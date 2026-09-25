// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

/// Error codes returned by [`MultisigContract`](crate::MultisigContract).
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultisigError {
    /// The contract has already been initialized.
    AlreadyInitialized = 1,
    /// The contract has not been initialized.
    NotInitialized = 2,
    /// The threshold must be greater than zero and no greater than total weight.
    InvalidThreshold = 3,
    /// Signer lists cannot be empty or contain duplicates.
    InvalidSigners = 4,
    /// The caller or approver is not a signer.
    NotSigner = 5,
    /// The transaction does not exist.
    TransactionNotFound = 6,
    /// The transaction has already been executed.
    AlreadyExecuted = 7,
    /// The signer has already signed the transaction.
    AlreadySigned = 8,
    /// The transaction has too few signatures to execute.
    ThresholdNotMet = 9,
    /// Signer-management approval list does not satisfy the threshold.
    InsufficientApprovals = 10,
    /// The proposal has expired and can no longer be signed or executed.
    ProposalExpired = 11,
    /// A signer weight of zero is not permitted.
    InvalidWeight = 12,
    /// `cleanup_expired` was called before the proposal's expiry ledger was reached.
    NotYetExpired = 13,
    /// A reentrant call was made while an external invocation was in progress.
    Reentrant = 14,
    /// The proposal is queued but its timelock delay has not yet elapsed.
    TimelockNotElapsed = 15,
    /// A timelock is configured and the proposal has not been queued yet.
    NotQueued = 16,
    /// The proposal has been cancelled.
    TransactionCancelled = 17,
    /// No daily spending allowance has been configured.
    SpendingNotConfigured = 18,
    /// The caller is not the configured spending operator.
    NotSpendingOperator = 19,
    /// The spend would exceed the daily allowance.
    DailyLimitExceeded = 20,
    /// Too many allowance spends in the current window.
    RateLimited = 21,
    /// Amounts must be positive (limits must be non-negative).
    InvalidAmount = 22,
    /// Only the original proposer may cancel a proposal.
    NotProposer = 14,
    /// The signer has not signed the proposal, so there is nothing to revoke.
    NotSigned = 15,
}

impl_display_error!(
    MultisigError,
    AlreadyInitialized  => "already initialized",
    NotInitialized      => "not initialized",
    InvalidThreshold    => "invalid threshold",
    InvalidSigners      => "invalid signers",
    NotSigner           => "not signer",
    TransactionNotFound => "transaction not found",
    AlreadyExecuted     => "already executed",
    AlreadySigned       => "already signed",
    ThresholdNotMet     => "threshold not met",
    InsufficientApprovals => "insufficient approvals",
    ProposalExpired     => "proposal expired",
    InvalidWeight       => "invalid weight",
    NotYetExpired       => "not yet expired",
    Reentrant           => "reentrant call",
    TimelockNotElapsed  => "timelock not elapsed",
    NotQueued           => "not queued",
    TransactionCancelled => "transaction cancelled",
    SpendingNotConfigured => "spending not configured",
    NotSpendingOperator => "not spending operator",
    DailyLimitExceeded  => "daily limit exceeded",
    RateLimited         => "rate limited",
    InvalidAmount       => "invalid amount",
    NotProposer         => "not proposer",
    NotSigned           => "not signed",
);
