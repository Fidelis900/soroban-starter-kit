#![allow(deprecated)]

use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

pub fn initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "initialized"),), admin.clone());
}

pub fn voter_registered(env: &Env, voter: &Address) {
    env.events()
        .publish((Symbol::new(env, "voter_registered"),), voter.clone());
}

pub fn voter_deregistered(env: &Env, voter: &Address) {
    env.events()
        .publish((Symbol::new(env, "voter_deregistered"),), voter.clone());
}

pub fn voted(env: &Env, voter: &Address, choice: u32) {
    env.events()
        .publish((Symbol::new(env, "voted"),), (voter.clone(), choice));
}

/// Emitted by `commit_vote` (#1125).
/// Only the commitment hash is published; the voter's choice stays secret
/// until the reveal phase, preventing bandwagoning and vote bribery.
pub fn vote_committed(env: &Env, voter: &Address, commitment: &BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "vote_committed"),),
        (voter.clone(), commitment.clone()),
    );
}

/// Emitted by `reveal_vote` (#1125).
/// Published only after the commitment is verified against the revealed
/// `(choice, salt)` preimage.
pub fn vote_revealed(env: &Env, voter: &Address, choice: u32) {
    env.events().publish(
        (Symbol::new(env, "vote_revealed"),),
        (voter.clone(), choice),
    );
}

/// Emitted by `commit_vote_blinded` (#1331).
/// Publishes only the blinded commitment hash, not the voter address.
pub fn blinded_committed(env: &Env, commitment: &BytesN<32>) {
    env.events()
        .publish((Symbol::new(env, "blinded_committed"),), commitment.clone());
}

/// Emitted by `reveal_vote_blinded` (#1331).
/// Publishes the revealed choice against its commitment hash without a voter
/// address, preserving reveal anonymity.
pub fn vote_revealed_blinded(env: &Env, commitment: &BytesN<32>, choice: u32) {
    env.events().publish(
        (Symbol::new(env, "vote_revealed_blinded"),),
        (commitment.clone(), choice),
    );
}

/// Emitted by `tally` (binary ballot, backward compat).
pub fn tally_result(env: &Env, yes: i128, no: i128) {
    env.events()
        .publish((Symbol::new(env, "tally_result"),), (yes, no));
}

/// Emitted when a ballot is closed by a tally (#1121).
pub fn tally_completed(env: &Env, counts: &Vec<i128>) {
    env.events()
        .publish((Symbol::new(env, "TallyCompleted"),), counts.clone());
}
