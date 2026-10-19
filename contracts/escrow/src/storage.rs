// #[contracttype] generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Env, contracttype};

/// Top-level storage keys used by [`EscrowContract`](crate::EscrowContract).
///
/// All keys are stored in instance storage so they share a single TTL bump.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The buyer's [`Address`].
    Buyer,
    /// The seller's [`Address`].
    Seller,
    /// The arbiter's [`Address`] (used for dispute resolution).
    Arbiter,
    /// The Soroban token contract [`Address`] used for fund transfers.
    TokenContract,
    /// Escrowed token amount as `i128`.
    Amount,
    /// Ledger sequence number after which a refund may be requested (`u32`).
    Deadline,
    /// Current [`EscrowState`] of the escrow lifecycle.
    State,
    /// Whether the contract is paused (`bool`).
    Paused,
    /// Contract version number (`u32`).
    Version,
    /// Pending WASM upgrade: `(BytesN<32>, u32)` = (hash, `ready_after_ledger`).
    PendingUpgrade,
    /// Multiple arbiters for multi-sig support (`Vec<Address>`).
    Arbiters,
    /// Number of required signatures for multi-sig resolution (`u32`).
    RequiredSignatures,
    /// Arbiter votes for dispute resolution to release to seller (`Vec<Address>`).
    ArbiterVotesRelease,
    /// Arbiter votes for dispute resolution to refund to buyer (`Vec<Address>`).
    ArbiterVotesRefund,
    /// Optional off-chain deal reference hash (`BytesN<32>`).
    MetadataHash,
    /// Number of ledgers after a dispute is raised before the buyer can auto-claim
    /// a refund without arbiter resolution (`u32`). Zero means no timeout.
    DisputeTimeoutLedgers,
    /// Ledger sequence at which the current dispute was raised (`u32`).
    DisputeRaisedAt,
    /// Grace period in ledgers after mark_delivered before buyer can claim expiration refund (`u32`).
    GracePeriodLedgers,
    /// Ledger sequence at which mark_delivered was called (`u32`).
    DeliveryMarkedAt,
    /// List of milestones for multi-milestone escrows (`Vec<Milestone>`).
    /// Only present when the escrow was initialized with milestones.
    Milestones,
    /// Fee in basis points (0–10 000) deducted from each released amount and
    /// routed to [`DataKey::Treasury`] (`u32`).
    FeeBps,
    /// Treasury [`Address`] that receives the deducted fee on each release.
    Treasury,
    /// Reentrancy guard flag (`bool`). Set to `true` while a token transfer is
    /// in progress; any reentrant call that hits [`require_not_locked`] will
    /// immediately return [`EscrowError::NotAuthorized`].
    Locked,
    /// Ordered list of token contract [`Address`]es for a multi-token escrow.
    /// Only present when the escrow was initialized with
    /// [`EscrowContract::initialize_with_tokens`].
    TokenContracts,
    /// Parallel list of amounts for each entry in [`DataKey::TokenContracts`].
    TokenAmounts,
    /// Number of ledgers after the deadline before an abandoned funded escrow
    /// can be refunded by its buyer.
    AbandonmentBuffer,
    /// Arbiter fee in basis points (0–2 000) deducted from the disputed amount
    /// and paid to the arbiter(s) upon dispute resolution (`Option<u32>`).
    /// `None` means no arbiter fee is charged.
    ArbiterFeeBps,
    /// Distinct arbiter approvals collected for the current dispute direction
    /// (`Vec<Address>`).  Each arbiter may approve at most once per direction;
    /// once the configured threshold is reached the dispute can be resolved.
    DisputeApprovals,
}

/// Lifecycle states of an escrow.
///
/// Transitions follow a strict order:
/// `Created → Funded → Delivered → Completed`
/// with side exits to `Refunded` or `Cancelled`.
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum EscrowState {
    /// Escrow has been initialized but not yet funded.
    Created = 0,
    /// Buyer has transferred tokens to the contract.
    Funded = 1,
    /// Seller has marked the obligation as delivered.
    Delivered = 2,
    /// Escrow is under arbiter review.
    Disputed = 3,
    /// Funds have been released to the seller.
    Completed = 4,
    /// Funds have been returned to the buyer.
    Refunded = 5,
    /// Escrow was cancelled before funding.
    Cancelled = 6,
    /// The buyer reclaimed funds after the abandonment window elapsed.
    ExpiredRefunded = 7,
}

impl Default for EscrowState {
    fn default() -> Self {
        EscrowState::Created
    }
}

impl core::fmt::Display for EscrowState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            EscrowState::Created => "created",
            EscrowState::Funded => "funded",
            EscrowState::Delivered => "delivered",
            EscrowState::Disputed => "disputed",
            EscrowState::Completed => "completed",
            EscrowState::Refunded => "refunded",
            EscrowState::Cancelled => "cancelled",
            EscrowState::ExpiredRefunded => "expired_refunded",
        })
    }
}

// ─── Reentrancy guard (#1190) ─────────────────────────────────────────────────

/// Acquire the reentrancy lock.
///
/// Sets [`DataKey::Locked`] to `true` in instance storage.  Must be paired
/// with a call to [`release_lock`] once the protected section (token transfer)
/// is complete.
///
/// # Errors
/// Returns [`EscrowError::NotAuthorized`] if the lock is already held,
/// i.e., a reentrant call is detected.
pub fn acquire_lock(env: &Env) -> Result<(), crate::errors::EscrowError> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Locked)
        .unwrap_or(false)
    {
        return Err(crate::errors::EscrowError::Reentrancy);
    }
    env.storage().instance().set(&DataKey::Locked, &true);
    Ok(())
}

/// Release the reentrancy lock acquired by [`acquire_lock`].
pub fn release_lock(env: &Env) {
    env.storage().instance().set(&DataKey::Locked, &false);
}

// ─── State guard ──────────────────────────────────────────────────────────────

/// Reads the current [`EscrowState`] from instance storage and returns
/// `Err(EscrowError::InvalidState)` when it does not match `expected`.
/// Returns `Err(EscrowError::NotInitialized)` when no state has been stored yet.
pub fn require_state(env: &Env, expected: EscrowState) -> Result<(), crate::errors::EscrowError> {
    let state: EscrowState = env
        .storage()
        .instance()
        .get(&DataKey::State)
        .ok_or(crate::errors::EscrowError::NotInitialized)?;
    if state != expected {
        return Err(crate::errors::EscrowError::InvalidState);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::{EscrowInfo, EscrowState};
    use std::string::ToString;

    /// XDR ABI snapshot for [`EscrowInfo`].
    ///
    /// `EscrowInfo` is serialised on-chain as an XDR map.  This test pins the
    /// set of field names (the map keys, sorted alphabetically) as a stored
    /// snapshot.  If this test fails after a struct change it means the on-chain
    /// ABI has changed and existing clients will break.
    #[test]
    fn test_escrow_info_xdr_snapshot() {
        use soroban_sdk::{
            Address, Env, IntoVal, TryFromVal,
            testutils::Address as _,
            xdr::{Limits, ScVal, ToXdr, WriteXdr},
        };

        let env = Env::default();
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let token_contract = Address::generate(&env);

        let info = EscrowInfo {
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            token_contract: token_contract.clone(),
            amount: 1_000i128,
            deadline: 500u32,
            state: EscrowState::Created,
            metadata_hash: soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
            arbiter_fee_bps: Some(250u32),
            arbiters: soroban_sdk::Vec::new(&env),
            threshold: 0u32,
        };

        // Round-trip: encode → decode must produce the same value.
        // A failure here means the XDR encoding changed and is no longer
        // self-consistent — that is always a breaking ABI change.
        let val: soroban_sdk::Val = info.clone().into_val(&env);
        let decoded =
            EscrowInfo::try_from_val(&env, &val).expect("EscrowInfo XDR round-trip failed");
        assert_eq!(
            decoded, info,
            "EscrowInfo XDR round-trip produced a different value"
        );

        // Structural snapshot: the XDR map must have exactly these keys,
        // in alphabetical order.  Hard-coded here as the stored snapshot.
        // Changing this list is a breaking on-chain ABI change.
        let sc_val = ScVal::try_from_val(&env, &val).expect("Val → ScVal failed");
        let keys: std::vec::Vec<std::string::String> = match &sc_val {
            ScVal::Map(Some(map)) => map
                .0
                .iter()
                .map(|entry| match &entry.key {
                    ScVal::Symbol(sym) => sym.0.to_utf8_string().expect("symbol is valid utf8"),
                    other => panic!("expected Symbol key, got {:?}", other),
                })
                .collect(),
            other => panic!("EscrowInfo must encode as ScVal::Map, got {:?}", other),
        };

        // Stored snapshot — DO NOT change without a migration plan.
        assert_eq!(
            keys,
            [
                "amount",
                "arbiter",
                "arbiter_fee_bps",
                "arbiters",
                "buyer",
                "deadline",
                "metadata_hash",
                "seller",
                "state",
                "threshold",
                "token_contract"
            ],
            "EscrowInfo XDR field names changed — breaking on-chain ABI"
        );
    }

    #[test]
    fn test_escrow_state_default() {
        assert_eq!(EscrowState::default(), EscrowState::Created);
    }

    #[test]
    fn test_escrow_state_display() {
        assert_eq!(EscrowState::Created.to_string(), "created");
        assert_eq!(EscrowState::Funded.to_string(), "funded");
        assert_eq!(EscrowState::Delivered.to_string(), "delivered");
        assert_eq!(EscrowState::Disputed.to_string(), "disputed");
        assert_eq!(EscrowState::Completed.to_string(), "completed");
        assert_eq!(EscrowState::Refunded.to_string(), "refunded");
        assert_eq!(EscrowState::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_escrow_state_ordering() {
        assert!(EscrowState::Created < EscrowState::Funded);
        assert!(EscrowState::Funded < EscrowState::Delivered);
        assert!(EscrowState::Delivered < EscrowState::Disputed);
        assert!(EscrowState::Funded >= EscrowState::Created);
        assert_eq!(EscrowState::Created, EscrowState::Created);
    }
}

#[cfg(test)]
mod discriminant_tests {
    use super::*;

    // In Soroban, #[contracttype] enums use the variant NAME as the XDR storage discriminant.
    // NEVER rename, reorder, or remove variants — doing so will corrupt on-chain storage for
    // any live deployment. To add a new key, append it at the END of the enum definition.
    //
    // This exhaustive match is the primary guard: it causes a COMPILE ERROR if a variant is
    // renamed or removed, and a non-exhaustive warning if one is added without updating here.
    fn escrow_data_key_index(key: &DataKey) -> u32 {
        match key {
            DataKey::Buyer => 0,
            DataKey::Seller => 1,
            DataKey::Arbiter => 2,
            DataKey::TokenContract => 3,
            DataKey::Amount => 4,
            DataKey::Deadline => 5,
            DataKey::State => 6,
            DataKey::Paused => 7,
            DataKey::Version => 8,
            DataKey::PendingUpgrade => 9,
            DataKey::Arbiters => 10,
            DataKey::RequiredSignatures => 11,
            DataKey::ArbiterVotesRelease => 12,
            DataKey::ArbiterVotesRefund => 13,
            DataKey::MetadataHash => 14,
            DataKey::DisputeTimeoutLedgers => 15,
            DataKey::DisputeRaisedAt => 16,
            DataKey::Milestones => 17,
            DataKey::FeeBps => 18,
            DataKey::Treasury => 19,
            DataKey::AbandonmentBuffer => 20,
            DataKey::GracePeriodLedgers => 17,
            DataKey::DeliveryMarkedAt => 18,
            DataKey::Milestones => 19,
            DataKey::FeeBps => 20,
            DataKey::Treasury => 21,
            DataKey::Locked => 22,
            DataKey::TokenContracts => 23,
            DataKey::TokenAmounts => 24,
            DataKey::ArbiterFeeBps => 25,
            DataKey::DisputeApprovals => 26,
        }
    }

    #[test]
    fn data_key_discriminants_are_stable() {
        assert_eq!(escrow_data_key_index(&DataKey::Buyer), 0);
        assert_eq!(escrow_data_key_index(&DataKey::Seller), 1);
        assert_eq!(escrow_data_key_index(&DataKey::Arbiter), 2);
        assert_eq!(escrow_data_key_index(&DataKey::TokenContract), 3);
        assert_eq!(escrow_data_key_index(&DataKey::Amount), 4);
        assert_eq!(escrow_data_key_index(&DataKey::Deadline), 5);
        assert_eq!(escrow_data_key_index(&DataKey::State), 6);
        assert_eq!(escrow_data_key_index(&DataKey::Paused), 7);
        assert_eq!(escrow_data_key_index(&DataKey::Version), 8);
        assert_eq!(escrow_data_key_index(&DataKey::PendingUpgrade), 9);
        assert_eq!(escrow_data_key_index(&DataKey::Arbiters), 10);
        assert_eq!(escrow_data_key_index(&DataKey::RequiredSignatures), 11);
        assert_eq!(escrow_data_key_index(&DataKey::ArbiterVotesRelease), 12);
        assert_eq!(escrow_data_key_index(&DataKey::ArbiterVotesRefund), 13);
        assert_eq!(escrow_data_key_index(&DataKey::MetadataHash), 14);
        assert_eq!(escrow_data_key_index(&DataKey::DisputeTimeoutLedgers), 15);
        assert_eq!(escrow_data_key_index(&DataKey::DisputeRaisedAt), 16);
        assert_eq!(escrow_data_key_index(&DataKey::Milestones), 17);
        assert_eq!(escrow_data_key_index(&DataKey::FeeBps), 18);
        assert_eq!(escrow_data_key_index(&DataKey::Treasury), 19);
        assert_eq!(escrow_data_key_index(&DataKey::AbandonmentBuffer), 20);
        assert_eq!(escrow_data_key_index(&DataKey::GracePeriodLedgers), 17);
        assert_eq!(escrow_data_key_index(&DataKey::DeliveryMarkedAt), 18);
        assert_eq!(escrow_data_key_index(&DataKey::Milestones), 19);
        assert_eq!(escrow_data_key_index(&DataKey::FeeBps), 20);
        assert_eq!(escrow_data_key_index(&DataKey::Treasury), 21);
        assert_eq!(escrow_data_key_index(&DataKey::Locked), 22);
        assert_eq!(escrow_data_key_index(&DataKey::TokenContracts), 23);
        assert_eq!(escrow_data_key_index(&DataKey::TokenAmounts), 24);
        assert_eq!(escrow_data_key_index(&DataKey::ArbiterFeeBps), 25);
        assert_eq!(escrow_data_key_index(&DataKey::DisputeApprovals), 26);
    }
}

/// A single work milestone within a multi-milestone escrow.
///
/// Each milestone holds a portion of the total escrowed funds and can be
/// released independently by the buyer (or an arbiter) once the corresponding
/// work is completed.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Milestone {
    /// Human-readable description of the deliverable (at most 64 bytes).
    pub description: soroban_sdk::String,
    /// Token amount reserved for this milestone.
    pub amount: i128,
    /// Whether this milestone's funds have already been released.
    pub released: bool,
}

/// Snapshot of all escrow fields returned by
/// [`EscrowContract::get_escrow_info`](crate::EscrowContract::get_escrow_info).
///
/// # ABI stability
///
/// `EscrowInfo` is serialised on-chain as an XDR map whose entries are keyed by
/// field name and sorted alphabetically.  The stable key order is:
///
/// `amount`, `arbiter`, `arbiter_fee_bps`, `arbiters`, `buyer`, `deadline`,
/// `metadata_hash`, `seller`, `state`, `threshold`, `token_contract`
///
/// **Renaming, adding, or removing any field is a breaking on-chain ABI change.**
/// Off-chain clients (SDKs, indexers, test harnesses) that decode this type from
/// raw XDR will break silently if the set of field names changes.  Any such change
/// must increment the on-chain contract version, provide a migration path, and be
/// called out explicitly in `CHANGELOG.md`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct EscrowInfo {
    /// Buyer address.
    pub buyer: Address,
    /// Seller address.
    pub seller: Address,
    /// Arbiter address.
    pub arbiter: Address,
    /// Token contract address.
    pub token_contract: Address,
    /// Current escrowed amount.
    pub amount: i128,
    /// Deadline ledger sequence number.
    pub deadline: u32,
    /// Current lifecycle state.
    pub state: EscrowState,
    /// Off-chain deal reference hash (32 bytes). All-zero means unset.
    ///
    /// Note: this is a fixed `BytesN<32>` rather than `Option<BytesN<32>>`
    /// because `soroban-sdk`'s `#[contracttype]` derive cannot generate the
    /// `testutils`-only `ScVal` conversions for `Option<BytesN<N>>` fields
    /// (`BytesN<N>` only implements the fallible `TryFrom<&BytesN<N>> for
    /// ScVal`, not the infallible `From` the generated `Option<T>` glue
    /// requires).
    pub metadata_hash: soroban_sdk::BytesN<32>,
    /// Arbiter fee in basis points (0–2 000) charged on dispute resolution.
    /// `None` means no arbiter fee is charged.
    pub arbiter_fee_bps: Option<u32>,
    /// Arbiter committee configured for this escrow (`Vec<Address>`).
    /// Empty when the escrow uses the single-arbiter model.
    pub arbiters: soroban_sdk::Vec<Address>,
    /// Number of distinct arbiter approvals required to resolve a dispute
    /// (`u32`). Zero when the escrow uses the single-arbiter model.
    pub threshold: u32,
}
