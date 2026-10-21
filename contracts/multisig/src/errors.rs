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
    /// Spending limit exceeded for the token.
    SpendingLimitExceeded = 14,
    /// Invalid spending limit configuration.
    InvalidSpendingLimit = 15,
}

impl_display_error!(
    MultisigError,
    AlreadyInitialized   => "already initialized",
    NotInitialized       => "not initialized",
    InvalidThreshold     => "invalid threshold",
    InvalidSigners       => "invalid signers",
    NotSigner            => "not signer",
    TransactionNotFound  => "transaction not found",
    AlreadyExecuted      => "already executed",
    AlreadySigned        => "already signed",
    ThresholdNotMet      => "threshold not met",
    InsufficientApprovals => "insufficient approvals",
    ProposalExpired      => "proposal expired",
    InvalidWeight        => "invalid weight",
    NotYetExpired        => "not yet expired",
    SpendingLimitExceeded => "spending limit exceeded",
    InvalidSpendingLimit  => "invalid spending limit",
);
