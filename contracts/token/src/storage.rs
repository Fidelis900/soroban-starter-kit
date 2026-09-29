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
        assert_eq!(token_data_key_index(&DataKey::PermitNonce(addr)), 14);
    }

    #[test]
    fn metadata_key_discriminants_are_stable() {
        assert_eq!(metadata_key_index(&MetadataKey::Name), 0);
        assert_eq!(metadata_key_index(&MetadataKey::Symbol), 1);
        assert_eq!(metadata_key_index(&MetadataKey::Decimals), 2);
    }
}
