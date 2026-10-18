use soroban_sdk::Env;

use crate::errors::EscrowError;
use crate::lifecycle::{extend_ttl, get_required};
use crate::storage::{DataKey, EscrowInfo, EscrowState};

use DataKey::*;

pub fn get_escrow_info(env: Env) -> Result<EscrowInfo, EscrowError> {
    Ok(EscrowInfo {
        buyer: get_required(&env, &Buyer)?,
        seller: get_required(&env, &Seller)?,
        arbiter: get_required(&env, &Arbiter)?,
        token_contract: get_required(&env, &TokenContract)?,
        amount: get_required(&env, &Amount)?,
        deadline: get_required(&env, &Deadline)?,
        state: get_required(&env, &State)?,
        metadata_hash: env
            .storage()
            .instance()
            .get(&MetadataHash)
            .unwrap_or_else(|| soroban_sdk::BytesN::from_array(&env, &[0u8; 32])),
        arbiter_fee_bps: env.storage().instance().get(&ArbiterFeeBps).unwrap_or(0),
        arbiters: env
            .storage()
            .instance()
            .get(&Arbiters)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env)),
        threshold: env.storage().instance().get(&Threshold).unwrap_or(0),
    })
}

pub fn get_state(env: Env) -> Option<EscrowState> {
    env.storage().instance().get(&State)
}

pub fn is_deadline_passed(env: Env) -> bool {
    let deadline: u32 = env.storage().instance().get(&Deadline).unwrap_or(0);
    env.ledger().sequence() > deadline
}

pub fn get_remaining_ledgers(env: Env) -> i64 {
    let deadline: u32 = env.storage().instance().get(&Deadline).unwrap_or(0);
    let current_sequence: u32 = env.ledger().sequence();
    deadline as i64 - current_sequence as i64
}

pub fn bump(env: Env) -> Result<(), EscrowError> {
    if !env.storage().instance().has(&State) {
        return Err(EscrowError::NotInitialized);
    }
    extend_ttl(&env);
    Ok(())
}