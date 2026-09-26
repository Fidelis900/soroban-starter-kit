//! Signature-based allowance approvals ("permit"), analogous to ERC-2612.
//!
//! Unlike Ethereum, where an address is derived directly from its public key,
//! Soroban's `Address` is opaque and cannot be derived from a raw ed25519 key
//! inside a contract. So an owner must first register the ed25519 public key
//! they intend to sign permits with via [`set_permit_signer`] — a single
//! owner-authenticated call. Every subsequent [`approve_with_signature`] call
//! is verified against that registered key, letting a spender submit an
//! owner-signed allowance without the owner ever broadcasting a transaction
//! themselves.
//!
//! Signed payload: `(contract_address, owner, spender, amount, nonce,
//! expiry_ledger)`, XDR-serialized via [`soroban_sdk::xdr::ToXdr`] and
//! verified with `env.crypto().ed25519_verify`. Including the contract
//! address in the payload prevents a permit signed for one deployment from
//! being replayed against another. `expiry_ledger` bounds how long the permit
//! may be submitted and, once applied, becomes the resulting allowance's
//! `expiration_ledger` — the same ledger-sequence expiration model `approve`
//! already uses. `nonce` must match the owner's current [`permit_nonce`] and
//! is incremented on success, so a consumed permit can never be replayed.
//!
//! An owner can also proactively invalidate an unused permit via
//! [`cancel_permit_nonce`], which marks a specific nonce as cancelled in
//! persistent storage. [`approve_with_signature`] rejects any permit whose
//! nonce has been cancelled, so a signature handed to an untrusted relayer can
//! be revoked without executing a dummy permit.

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env, panic_with_error};

use crate::allowance::set_allowance;
use crate::errors::TokenError;
use crate::events;
use crate::storage::DataKey;
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_persistent};

/// Registers (or rotates) the ed25519 public key `owner` will sign permits with.
pub fn set_permit_signer(env: Env, owner: Address, public_key: BytesN<32>) {
    owner.require_auth();
    let key = DataKey::PermitSigner(owner.clone());
    env.storage().persistent().set(&key, &public_key);
    extend_ttl_persistent(&env, &key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    events::permit_signer_set(&env, &owner);
}

/// Returns the ed25519 public key registered for `owner`'s permits, if any.
pub fn permit_signer(env: Env, owner: Address) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::PermitSigner(owner))
}

/// Returns the nonce `owner` must use in their next signed permit message.
pub fn permit_nonce(env: Env, owner: Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::PermitNonce(owner))
        .unwrap_or(0u32)
}

/// Returns `true` if `owner` has cancelled `nonce`, meaning any permit signed
/// for that nonce can no longer be applied.
pub fn is_permit_nonce_cancelled(env: Env, owner: Address, nonce: u32) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::CancelledPermitNonce(owner, nonce))
        .unwrap_or(false)
}

/// Cancels `nonce` for `owner`, permanently invalidating any permit signature
/// that was signed for it.
///
/// This lets an owner revoke an unused off-chain permit (for example one handed
/// to an untrusted relayer) without having to execute a dummy permit. The
/// cancellation is recorded in persistent storage and is checked by
/// [`approve_with_signature`], which rejects any permit whose nonce has been
/// cancelled.
pub fn cancel_permit_nonce(env: Env, owner: Address, nonce: u32) {
    owner.require_auth();
    let key = DataKey::CancelledPermitNonce(owner.clone(), nonce);
    env.storage().persistent().set(&key, &true);
    extend_ttl_persistent(&env, &key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    events::permit_nonce_cancelled(&env, &owner, nonce);
}

/// Grants `spender` an allowance over `owner`'s tokens using an owner-signed
/// message instead of the owner submitting the transaction themselves.
///
/// # Errors
/// - [`TokenError::PermitExpired`] if the current ledger is past `expiry_ledger`.
/// - [`TokenError::InvalidNonce`] if `nonce` doesn't match the owner's current nonce.
/// - [`TokenError::PermitSignerNotSet`] if `owner` never called `set_permit_signer`.
///
/// # Panics
/// If `signature` is not a valid ed25519 signature over the expected payload.
pub fn approve_with_signature(
    env: Env,
    owner: Address,
    spender: Address,
    amount: i128,
    nonce: u32,
    expiry_ledger: u32,
    signature: BytesN<64>,
) -> Result<(), TokenError> {
    #[cfg(feature = "pausable")]
    {
        use crate::require_not_paused;
        require_not_paused(&env)?;
    }

    if env.ledger().sequence() > expiry_ledger {
        return Err(TokenError::PermitExpired);
    }

    let nonce_key = DataKey::PermitNonce(owner.clone());
    let expected_nonce: u32 = env.storage().persistent().get(&nonce_key).unwrap_or(0);
    if nonce != expected_nonce {
        return Err(TokenError::InvalidNonce);
    }

    // A cancelled nonce is permanently invalid, even if it is still the
    // owner's current expected nonce.
    if is_permit_nonce_cancelled(env.clone(), owner.clone(), nonce) {
        return Err(TokenError::InvalidNonce);
    }

    let public_key: BytesN<32> = env
        .storage()
        .persistent()
        .get(&DataKey::PermitSigner(owner.clone()))
        .ok_or(TokenError::PermitSignerNotSet)?;

    let message: Bytes = (
        env.current_contract_address(),
        owner.clone(),
        spender.clone(),
        amount,
        nonce,
        expiry_ledger,
    )
        .to_xdr(&env);

    // Host-level panic if the signature doesn't verify; there is no soft
    // error path for a cryptographic verification failure.
    env.crypto()
        .ed25519_verify(&public_key, &message, &signature);

    let next_nonce = match expected_nonce.checked_add(1) {
        Some(n) => n,
        None => panic_with_error!(&env, TokenError::Overflow),
    };
    env.storage().persistent().set(&nonce_key, &next_nonce);
    extend_ttl_persistent(
        &env,
        &nonce_key,
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );

    set_allowance(&env, owner.clone(), spender.clone(), amount, expiry_ledger);
    if amount == 0 {
        events::revoked(&env, &owner, &spender);
    } else {
        events::approved(&env, &owner, &spender, amount);
    }
    events::permit_used(&env, &owner, &spender, amount, nonce);
    Ok(())
}
