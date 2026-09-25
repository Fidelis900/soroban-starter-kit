use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

pub fn initialized(env: &Env, admin: &Address) {
    env.events().publish((Symbol::new(env, "initialized"),), admin.clone());
}

/// Emitted by `initialize` (#1123).
/// `quadratic` indicates whether quadratic voting mode is enabled.
pub fn initialized_with_mode(env: &Env, admin: &Address, quadratic: bool) {
    env.events().publish(
        (Symbol::new(env, "initialized"),),
        (admin.clone(), quadratic),
    );
}

pub fn voter_registered(env: &Env, voter: &Address) {
    env.events()
        .publish((Symbol::new(env, "voter_registered"),), voter.clone());
}

/// Emitted by `register_voters_batch` (#1122).
/// `voters` contains the addresses newly registered in the batch.
pub fn voters_registered_batch(env: &Env, voters: &Vec<Address>) {
    env.events()
        .publish((Symbol::new(env, "voters_registered_batch"),), voters.clone());
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

/// Emitted by `tally` (binary ballot, backward compat).
pub fn tally_result(env: &Env, yes: i128, no: i128) {
    env.events()
        .publish((Symbol::new(env, "tally_result"),), (yes, no));
}

/// Emitted by `tally_all` (multi-choice ballot, #788).
/// `counts` contains per-choice vote tallies in declaration order.
pub fn tally_all_result(env: &Env, counts: &Vec<i128>) {
    env.events()
        .publish((Symbol::new(env, "tally_all_result"),), counts.clone());
}

/// Emitted by `tally`/`tally_all` when quadratic mode is active (#1123).
/// `counts` contains per-choice tallies computed from the integer square
/// root of each voter's token weight.
pub fn tally_quadratic_result(env: &Env, counts: &Vec<i128>) {
    env.events()
        .publish((Symbol::new(env, "tally_quadratic_result"),), counts.clone());
/// Emitted when the ballot is closed via `tally_all` after the voting window
/// has ended (#1121). Signals that official tally results are final.
pub fn tally_completed(env: &Env, counts: &Vec<i128>) {
    env.events()
        .publish((Symbol::new(env, "TallyCompleted"),), counts.clone());
}
