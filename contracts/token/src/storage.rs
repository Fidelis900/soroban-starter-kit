use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// Instance storage – pending admin address for two-step admin transfer.
    PendingAdmin,
    /// Persistent storage – token balance (`i128`) for a given [`Address`].
    Balance(Address),
    Allowance(AllowanceDataKey),
    Metadata(MetadataKey),
    TotalSupply,
    /// Instance storage – whether the contract is paused (`bool`).
    Paused,
    /// Instance storage – maximum tokens that may ever be minted (`i128`).
    MaxSupply,
    /// Instance storage – pending WASM upgrade: `(BytesN<32>, u32)` = (hash, ready_after_ledger).
    PendingUpgrade,
    /// Instance storage – contract version number (`u32`).
    Version,
    /// Persistent storage – frozen accounts set (`bool` per address).
    Frozen(Address),
    /// Instance storage – optional transfer hook contract address (`Address`).
    TransferHook,
    /// Persistent storage – balance snapshot: key is `(Address, ledger: u32)` → `i128`.
    Snapshot(Address, u32),
    /// Persistent storage – ed25519 public key registered for an owner's
    /// signature-based permits (`BytesN<32>`).
    PermitSigner(Address),
    /// Persistent storage – next expected nonce for an owner's signature-based
    /// permits (`u32`).
    PermitNonce(Address),
    /// Persistent storage – nonces explicitly cancelled by an owner so that a
    /// previously signed permit can no longer be applied (`bool` per
    /// `(Address, u32)`).
    CancelledPermitNonce(Address, u32),
    /// Persistent storage – permanently blacklisted accounts (`bool` per address).
    /// Distinct from temporary [`DataKey::Frozen`] investigative freezes (issue #1178).
    Blacklisted(Address),
    /// Instance storage – compliance treasury address that receives confiscated
    /// balances from blacklisted accounts (`Address`).
    ComplianceTreasury,
    /// Persistent storage – sorted vector of `(ledger, balance)` snapshot
    /// entries for a given account, used for O(log N) binary-search lookups
    /// by `balance_at` (issue #1175).
    SnapshotHistory(Address),
}

#[contracttype]
#[derive(Clone)]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum MetadataKey {
    Name,
    Symbol,
    Decimals,
}

/// A single historical balance snapshot entry stored in a per-account vector.
///
/// Entries are kept sorted by `ledger` in ascending order (monotonically
/// increasing because snapshots are taken at successively later ledgers),
/// enabling O(log N) binary search in `balance_at` (issue #1175).
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotEntry {
    /// The ledger sequence at which the snapshot was taken.
    pub ledger: u32,
    /// The account balance at that ledger.
    pub balance: i128,
}

#[cfg(test)]
mod discriminant_tests {
    use super::*;
    use soroban_sdk::{Address, Env, testutils::Address as _};

    // In Soroban, #[contracttype] enums use the variant NAME as the XDR storage discriminant.
    // NEVER rename, reorder, or remove variants — doing so will corrupt on-chain storage for
    // any live deployment. To add a new key, append it at the END of the enum definition.
    //
    // This exhaustive match is the primary guard: it causes a COMPILE ERROR if a variant is
    // renamed or removed, and a non-exhaustive warning if one is added without updating here.
    fn token_data_key_index(key: &DataKey) -> u32 {
        match key {
            DataKey::Admin => 0,
            DataKey::PendingAdmin => 1,
            DataKey::Balance(_) => 2,
            DataKey::Allowance(_) => 3,
            DataKey::Metadata(_) => 4,
            DataKey::TotalSupply => 5,
            DataKey::Paused => 6,
            DataKey::MaxSupply => 7,
            DataKey::PendingUpgrade => 8,
            DataKey::Version => 9,
            DataKey::Frozen(_) => 10,
            DataKey::TransferHook => 11,
            DataKey::Snapshot(_, _) => 12,
            DataKey::PermitSigner(_) => 13,
            DataKey::PermitNonce(_) => 14,
            DataKey::CancelledPermitNonce(_, _) => 15,
            DataKey::Blacklisted(_) => 16,
            DataKey::ComplianceTreasury => 17,
            DataKey::SnapshotHistory(_) => 18,
        }
    }

    fn metadata_key_index(key: &MetadataKey) -> u32 {
        match key {
            MetadataKey::Name => 0,
            MetadataKey::Symbol => 1,
            MetadataKey::Decimals => 2,
        }
    }

    #[test]
    fn data_key_discriminants_are_stable() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let allowance_key = AllowanceDataKey {
            from: addr.clone(),
            spender: addr.clone(),
        };

        assert_eq!(token_data_key_index(&DataKey::Admin), 0);
        assert_eq!(token_data_key_index(&DataKey::PendingAdmin), 1);
        assert_eq!(token_data_key_index(&DataKey::Balance(addr.clone())), 2);
        assert_eq!(token_data_key_index(&DataKey::Allowance(allowance_key)), 3);
        assert_eq!(
            token_data_key_index(&DataKey::Metadata(MetadataKey::Name)),
            4
        );
        assert_eq!(token_data_key_index(&DataKey::TotalSupply), 5);
        assert_eq!(token_data_key_index(&DataKey::Paused), 6);
        assert_eq!(token_data_key_index(&DataKey::MaxSupply), 7);
        assert_eq!(token_data_key_index(&DataKey::PendingUpgrade), 8);
        assert_eq!(token_data_key_index(&DataKey::Version), 9);
        assert_eq!(token_data_key_index(&DataKey::Frozen(addr.clone())), 10);
        assert_eq!(token_data_key_index(&DataKey::TransferHook), 11);
        assert_eq!(
            token_data_key_index(&DataKey::Snapshot(Address::generate(&env), 0u32)),
            12
        );
        assert_eq!(
            token_data_key_index(&DataKey::PermitSigner(addr.clone())),
            13
        );
        assert_eq!(token_data_key_index(&DataKey::PermitNonce(addr.clone())), 14);
        assert_eq!(
            token_data_key_index(&DataKey::CancelledPermitNonce(addr.clone(), 0u32)),
            15
        );
        assert_eq!(token_data_key_index(&DataKey::Blacklisted(addr)), 16);
        assert_eq!(token_data_key_index(&DataKey::ComplianceTreasury), 17);
        assert_eq!(
            token_data_key_index(&DataKey::SnapshotHistory(Address::generate(&env))),
            18
        );
    }

    #[test]
    fn metadata_key_discriminants_are_stable() {
        assert_eq!(metadata_key_index(&MetadataKey::Name), 0);
        assert_eq!(metadata_key_index(&MetadataKey::Symbol), 1);
        assert_eq!(metadata_key_index(&MetadataKey::Decimals), 2);
    }
}
