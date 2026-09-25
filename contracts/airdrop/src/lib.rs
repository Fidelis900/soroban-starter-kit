#![no_std]
#![deny(missing_docs)]
//! Merkle-proof airdrop contract template.
//!
//! An admin sets a merkle root describing the distribution; eligible accounts
//! claim their allocation by presenting a merkle proof. Duplicate claims are
//! rejected on-chain.

use soroban_sdk::{Address, Bytes, BytesN, Env, Vec, contract, contractimpl, token, xdr::ToXdr};

mod errors;
mod events;
mod storage;

pub use errors::AirdropError;
pub use storage::DataKey;

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_claimed(env: &Env, round_id: u32, recipient: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Claimed(round_id, recipient.clone()),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

/// Compute the merkle leaf: sha256(recipient_bytes || amount_be_bytes).
fn compute_leaf(env: &Env, recipient: &Address, amount: i128) -> BytesN<32> {
    let mut data = Bytes::new(env);
    // Encode recipient as its XDR/SC address bytes via to_xdr equivalent.
    // We use the address bytes representation available via soroban's Bytes conversion.
    let addr_bytes = recipient.clone().to_xdr(env);
    data.append(&addr_bytes);
    // Encode amount as 16-byte big-endian.
    let amount_bytes: [u8; 16] = amount.to_be_bytes();
    data.append(&Bytes::from_slice(env, &amount_bytes));
    env.crypto().sha256(&data).into()
}

/// Sort-and-hash two nodes (standard sorted-pair merkle tree).
fn hash_pair(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::new(env);
    // Sort to make the tree order-independent.
    if a.to_array() <= b.to_array() {
        data.append(&Bytes::from(a.clone()));
        data.append(&Bytes::from(b.clone()));
    } else {
        data.append(&Bytes::from(b.clone()));
        data.append(&Bytes::from(a.clone()));
    }
    env.crypto().sha256(&data).into()
}

/// Verify a merkle proof.
///
/// `proof` is the list of sibling hashes from leaf to root.
/// `root` is the expected merkle root.
fn verify_proof(env: &Env, leaf: BytesN<32>, proof: &Vec<BytesN<32>>, root: &BytesN<32>) -> bool {
    let mut current = leaf;
    for sibling in proof.iter() {
        current = hash_pair(env, &current, &sibling);
    }
    &current == root
}

/// Merkle-proof airdrop contract.
///
/// Lifecycle:
/// 1. Admin calls `initialize` to set the token address.
/// 2. Admin calls `set_root` with the merkle root of the airdrop distribution tree.
/// 3. Each eligible address calls `claim(round_id, recipient, amount, proof)` with a
///    pre-computed merkle proof, or signs an off-chain authorization that a sponsor
///    submits via `claim_for`. Duplicate claims are rejected on-chain.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct AirdropContract;

    #[contractimpl]
    impl AirdropContract {
        /// Initialize the airdrop contract.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::AlreadyInitialized`] if already initialized.
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
            claim_deadline: u32,
        ) -> Result<(), AirdropError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(AirdropError::AlreadyInitialized);
            }

            admin.require_auth();

            // Validate token interface.
            token::Client::new(&env, &token).decimals();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage()
                .instance()
                .set(&DataKey::ClaimDeadline, &claim_deadline);
            bump_instance(&env);
            Ok(())
        }

        /// Set (or replace) the merkle root for a given round. Only the admin may call this.
        ///
        /// The root of an active, unexpired round cannot be replaced: once a round's
        /// root is set and its claim window is still open, it is immutable. This
        /// prevents an admin from invalidating outstanding proofs mid-round.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if the contract has not been initialized.
        /// Returns [`AirdropError::Unauthorized`] if caller is not the admin.
        /// Returns [`AirdropError::RoundActive`] if the round already has a root and is unexpired.
        pub fn set_root(
            env: Env,
            round_id: u32,
            root: BytesN<32>,
        ) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;

            admin.require_auth();

            // Prevent changing the root of an active, unexpired round.
            let existing: Option<Bytes> = env
                .storage()
                .instance()
                .get(&DataKey::MerkleRoot(round_id));
            if existing.is_some() {
                let deadline: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ClaimDeadline)
                    .ok_or(AirdropError::NotInitialized)?;
                if env.ledger().sequence() <= deadline {
                    return Err(AirdropError::RoundActive);
                }
            }

            let root_bytes = Bytes::from(root.clone());
            env.storage()
                .instance()
                .set(&DataKey::MerkleRoot(round_id), &root_bytes);
            bump_instance(&env);

            events::root_set(&env, round_id, &root_bytes);
            Ok(())
        }

        /// Claim tokens by supplying a valid merkle proof for a given round.
        ///
        /// The caller must appear in the round's airdrop tree with exactly `amount` tokens.
        /// Claims are tracked per `(round_id, recipient)`, so a recipient may claim
        /// once in each round of a multi-round campaign.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set for the round.
        /// Returns [`AirdropError::InvalidAmount`] if `amount <= 0`.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        /// Returns [`AirdropError::AlreadyClaimed`] if the address already claimed in this round.
        /// Returns [`AirdropError::InvalidProof`] if the merkle proof does not verify.
        pub fn claim(
            env: Env,
            round_id: u32,
            recipient: Address,
            amount: i128,
            proof: Vec<BytesN<32>>,
        ) -> Result<(), AirdropError> {
            recipient.require_auth();
            execute_claim(&env, round_id, &recipient, amount, &proof)?;
            events::claimed(&env, round_id, &recipient, amount);
            Ok(())
        }

        /// Claim on behalf of `recipient` using an off-chain ed25519 signature.
        ///
        /// Lets a relayer or sponsor submit (and pay the fee for) a claim for a
        /// recipient who holds no native XLM. The recipient signs the payload
        /// returned by [`claim_for_payload`](Self::claim_for_payload) with the
        /// ed25519 key behind their `G...` account address; tokens are always
        /// delivered to `recipient`, never to the submitter.
        ///
        /// Replay protection: the signed payload is bound to the network, this
        /// contract, the round, the recipient and the amount, and each
        /// `(round_id, recipient)` pair can be claimed at most once.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::InvalidSignature`] if `recipient` is not an
        /// ed25519 account address. Otherwise returns the same errors as
        /// [`claim`](Self::claim).
        ///
        /// # Panics
        ///
        /// Traps if `signature` does not verify against the recipient's public key.
        pub fn claim_for(
            env: Env,
            round_id: u32,
            recipient: Address,
            amount: i128,
            proof: Vec<BytesN<32>>,
            signature: BytesN<64>,
        ) -> Result<(), AirdropError> {
            let public_key = account_public_key(&env, &recipient)?;
            let payload = claim_for_payload(&env, round_id, &recipient, amount);
            env.crypto().ed25519_verify(&public_key, &payload, &signature);

            execute_claim(&env, round_id, &recipient, amount, &proof)?;
            events::claimed_for(&env, round_id, &recipient, amount);
            Ok(())
        }

        /// Return the exact bytes a recipient must sign to authorize [`claim_for`](Self::claim_for).
        pub fn claim_for_payload(env: Env, round_id: u32, recipient: Address, amount: i128) -> Bytes {
            claim_for_payload(&env, round_id, &recipient, amount)
        }

        /// Sweep any tokens left unclaimed after the claim deadline.
        ///
        /// Only the admin may call this, and only once the claim window has
        /// closed (`env.ledger().sequence() > claim_deadline`). The entire
        /// remaining token balance held by the contract is transferred to
        /// `recipient`, preventing funds from being permanently locked.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::ClaimWindowNotClosed`] if the deadline has not passed.
        /// Returns [`AirdropError::NothingToSweep`] if the contract holds no tokens.
        pub fn sweep_unclaimed(env: Env, recipient: Address) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;

            admin.require_auth();

            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(AirdropError::NotInitialized)?;

            let deadline: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ClaimDeadline)
                .ok_or(AirdropError::NotInitialized)?;
            if env.ledger().sequence() <= deadline {
                return Err(AirdropError::ClaimWindowNotClosed);
            }

            let token_client = token::Client::new(&env, &token_addr);
            let balance = token_client.balance(&env.current_contract_address());
            if balance <= 0 {
                return Err(AirdropError::NothingToSweep);
            }

            token_client.transfer(&env.current_contract_address(), &recipient, &balance);
            bump_instance(&env);

            events::unclaimed_swept(&env, &recipient, balance);
            Ok(())
        }

        /// Claim for many recipients in one transaction, skipping invalid entries.
        ///
        /// Entries with a non-positive amount, an invalid proof, or a recipient that
        /// has already claimed this round are skipped rather than reverting the batch.
        /// Returns the recipients that were successfully paid.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set for the round.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        pub fn claim_batch_lenient(
            env: Env,
            round_id: u32,
            claims: Vec<(Address, i128, Vec<BytesN<32>>)>,
        ) -> Result<Vec<Address>, AirdropError> {
            let (token_addr, root) = load_claim_context(&env, round_id)?;
            let token_client = token::Client::new(&env, &token_addr);

            let mut claimed = Vec::new(&env);
            for (recipient, amount, proof) in claims.iter() {
                if amount <= 0 {
                    continue;
                }
                let claimed_key = DataKey::Claimed(round_id, recipient.clone());
                if env.storage().persistent().has(&claimed_key) {
                    continue;
                }
                let leaf = compute_leaf(&env, &recipient, amount);
                if !verify_proof(&env, leaf, &proof, &root) {
                    continue;
                }

                env.storage().persistent().set(&claimed_key, &true);
                bump_claimed(&env, round_id, &recipient);
                token_client.transfer(&env.current_contract_address(), &recipient, &amount);

                events::batch_claimed(&env, round_id, &recipient, amount);
                claimed.push_back(recipient);
            }

            bump_instance(&env);
            Ok(claimed)
        }

        /// Returns `true` if `address` has already claimed in `round_id`.
        pub fn is_claimed(env: Env, round_id: u32, address: Address) -> bool {
            env.storage()
                .persistent()
                .get::<_, bool>(&DataKey::Claimed(round_id, address))
                .unwrap_or(false)
        }

        /// Returns the merkle root for `round_id`, or `None` if not set.
        pub fn get_root(env: Env, round_id: u32) -> Option<Bytes> {
            env.storage().instance().get(&DataKey::MerkleRoot(round_id))
        }
    }
}

/// Load the token address and merkle root for a round, rejecting claims after the deadline.
fn load_claim_context(env: &Env, round_id: u32) -> Result<(Address, BytesN<32>), AirdropError> {
    let token_addr: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(AirdropError::NotInitialized)?;

    let root_bytes: Bytes = env
        .storage()
        .instance()
        .get(&DataKey::MerkleRoot(round_id))
        .ok_or(AirdropError::RootNotSet)?;

    let deadline: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ClaimDeadline)
        .ok_or(AirdropError::NotInitialized)?;
    if env.ledger().sequence() > deadline {
        return Err(AirdropError::ClaimWindowClosed);
    }

    let root: BytesN<32> = root_bytes
        .try_into()
        .map_err(|_| AirdropError::RootNotSet)?;
    Ok((token_addr, root))
}

/// Verify and settle a single claim. The caller is responsible for authorizing `recipient`.
fn execute_claim(
    env: &Env,
    round_id: u32,
    recipient: &Address,
    amount: i128,
    proof: &Vec<BytesN<32>>,
) -> Result<(), AirdropError> {
    if amount <= 0 {
        return Err(AirdropError::InvalidAmount);
    }

    let (token_addr, root) = load_claim_context(env, round_id)?;

    // Duplicate-claim prevention, scoped to this round.
    let claimed_key = DataKey::Claimed(round_id, recipient.clone());
    if env.storage().persistent().has(&claimed_key) {
        return Err(AirdropError::AlreadyClaimed);
    }

    let leaf = compute_leaf(env, recipient, amount);
    if !verify_proof(env, leaf, proof, &root) {
        return Err(AirdropError::InvalidProof);
    }

    // Checks-effects-interactions: mark claimed before transfer.
    env.storage().persistent().set(&claimed_key, &true);
    bump_claimed(env, round_id, recipient);
    bump_instance(env);

    token::Client::new(env, &token_addr).transfer(
        &env.current_contract_address(),
        recipient,
        &amount,
    );
    Ok(())
}

/// Domain-separation tag for [`AirdropContract::claim_for`] payloads.
const CLAIM_FOR_DOMAIN: &[u8] = b"soroban-airdrop:claim_for:v1";

/// Build the payload a recipient signs to authorize a sponsored claim:
/// `domain || network_id || contract_xdr || round_id_be || recipient_xdr || amount_be`.
fn claim_for_payload(env: &Env, round_id: u32, recipient: &Address, amount: i128) -> Bytes {
    let mut payload = Bytes::from_slice(env, CLAIM_FOR_DOMAIN);
    payload.append(&Bytes::from(env.ledger().network_id()));
    payload.append(&env.current_contract_address().to_xdr(env));
    payload.append(&Bytes::from_slice(env, &round_id.to_be_bytes()));
    payload.append(&recipient.clone().to_xdr(env));
    payload.append(&Bytes::from_slice(env, &amount.to_be_bytes()));
    payload
}

/// XDR prefix of an `ScVal::Address(ScAddress::Account(PublicKey::Ed25519(..)))`:
/// ScVal type 18 (address), ScAddress type 0 (account), PublicKey type 0 (ed25519).
const ACCOUNT_ED25519_XDR_PREFIX: [u8; 12] = [0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0, 0];
const ACCOUNT_ED25519_XDR_PREFIX_LEN: u32 = 12;
const ACCOUNT_ED25519_XDR_LEN: u32 = ACCOUNT_ED25519_XDR_PREFIX_LEN + 32;

/// Extract the ed25519 public key behind a `G...` account address.
fn account_public_key(env: &Env, address: &Address) -> Result<BytesN<32>, AirdropError> {
    let xdr = address.clone().to_xdr(env);
    if xdr.len() != ACCOUNT_ED25519_XDR_LEN
        || xdr.slice(..ACCOUNT_ED25519_XDR_PREFIX_LEN)
            != Bytes::from_array(env, &ACCOUNT_ED25519_XDR_PREFIX)
    {
        return Err(AirdropError::InvalidSignature);
    }
    xdr.slice(ACCOUNT_ED25519_XDR_PREFIX_LEN..)
        .try_into()
        .map_err(|_| AirdropError::InvalidSignature)
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;

