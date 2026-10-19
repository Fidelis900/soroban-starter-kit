use soroban_sdk::{Address, Env, String, Symbol};

/// Emitted when the token is initialized.
/// Topics: (Symbol, Address) — event name, admin
pub fn initialized(env: &Env, admin: &Address, name: String, symbol: String, decimals: u32) {
    env.events().publish(
        (Symbol::new(env, "initialized"), admin.clone()),
        (name, symbol, decimals),
    );
}

/// Emitted when tokens are minted.
/// Topics: (Symbol, Address) — event name, recipient
pub fn minted(env: &Env, to: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "mint"), to.clone()), amount);
}

/// Emitted when tokens are burned.
/// Topics: (Symbol, Address) — event name, account
pub fn burned(env: &Env, from: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "burn"), from.clone()), amount);
}

/// Emitted when the admin is changed.
/// Topics: (Symbol, Address) — event name, old admin
pub fn admin_changed(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_changed"), old_admin.clone()),
        new_admin.clone(),
    );
}

/// Emitted when a new admin is proposed (two-step ownership transfer).
/// Topics: (Symbol, Address) — event name, current admin
/// Data: proposed (pending) admin
pub fn admin_proposed(env: &Env, current_admin: &Address, pending_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_proposed"), current_admin.clone()),
        pending_admin.clone(),
    );
}

/// Emitted when the pending admin accepts ownership.
/// Topics: (Symbol, Address) — event name, new admin
pub fn admin_accepted(env: &Env, new_admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "admin_accepted"), new_admin.clone()), ());
}

/// Emitted when a pending admin proposal is cancelled by the current admin.
/// Topics: (Symbol, Address) — event name, admin
pub fn admin_proposal_cancelled(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_proposal_cancelled"), admin.clone()),
        (),
    );
}

/// Emitted when a transfer lockup deadline is set for an account.
/// Topics: (Symbol, Address) — event name, account
/// Data: lock ledger (transfers are rejected until this ledger sequence)
pub fn transfer_lock_set(env: &Env, account: &Address, lock_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "transfer_lock_set"), account.clone()),
        lock_ledger,
    );
}

/// Emitted when an allowance is increased.
/// Topics: (Symbol, Address, Address) — event name, owner, spender
/// Data: (delta, new allowance)
pub fn allowance_increased(
    env: &Env,
    from: &Address,
    spender: &Address,
    delta: i128,
    new_allowance: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "allowance_increased"),
            from.clone(),
            spender.clone(),
        ),
        (delta, new_allowance),
    );
}

/// Emitted when an allowance is decreased.
/// Topics: (Symbol, Address, Address) — event name, owner, spender
/// Data: (delta, new allowance)
pub fn allowance_decreased(
    env: &Env,
    from: &Address,
    spender: &Address,
    delta: i128,
    new_allowance: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "allowance_decreased"),
            from.clone(),
            spender.clone(),
        ),
        (delta, new_allowance),
    );
}

pub fn approved(env: &Env, from: &Address, spender: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "approve"), from.clone(), spender.clone()),
        amount,
    );
}

/// Emitted when an allowance is revoked.
/// Topics: (Symbol, Address, Address) — event name, owner, spender
pub fn revoked(env: &Env, from: &Address, spender: &Address) {
    env.events().publish(
        (Symbol::new(env, "revoke"), from.clone(), spender.clone()),
        (),
    );
}

/// Emitted when tokens are transferred.
/// Topics: (Symbol, Address, Address) — event name, from, to
pub fn transferred(env: &Env, from: &Address, to: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "transfer"), from.clone(), to.clone()),
        amount,
    );
}

/// Emitted when tokens are batch transferred.
/// Topics: (Symbol, Address) — event name, from
/// Data: Vec<(Address, i128)> — recipients and amounts
pub fn transferred_batch(env: &Env, from: &Address, transfers: &soroban_sdk::Vec<(Address, i128)>) {
    env.events().publish(
        (Symbol::new(env, "transfer_batch"), from.clone()),
        transfers.clone(),
    );
}

/// Emitted for each individual recipient credit performed by
/// `transfer_to_many`.
/// Topics: (Symbol, Address, Address) — event name, from, to
/// Data: amount
///
/// This mirrors the standard `transferred` event shape so that indexers can
/// process multi-recipient payouts without special-casing them.
pub fn transferred_many(env: &Env, from: &Address, to: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "transfer"), from.clone(), to.clone()),
        amount,
    );
}

/// Emitted when an account is frozen.
/// Topics: (Symbol, Address) — event name, account
pub fn account_frozen(env: &Env, account: &Address) {
    env.events()
        .publish((Symbol::new(env, "account_frozen"), account.clone()), ());
}

/// Emitted when an account is unfrozen.
/// Topics: (Symbol, Address) — event name, account
pub fn account_unfrozen(env: &Env, account: &Address) {
    env.events()
        .publish((Symbol::new(env, "account_unfrozen"), account.clone()), ());
}

/// Emitted when an account is permanently blacklisted.
/// Topics: (Symbol, Address) — event name, account
pub fn account_blacklisted(env: &Env, account: &Address) {
    env.events().publish(
        (Symbol::new(env, "account_blacklisted"), account.clone()),
        (),
    );
}

/// Emitted when an account is removed from the blacklist.
/// Topics: (Symbol, Address) — event name, account
pub fn account_unblacklisted(env: &Env, account: &Address) {
    env.events().publish(
        (Symbol::new(env, "account_unblacklisted"), account.clone()),
        (),
    );
}

/// Emitted when blacklisted funds are confiscated to the compliance treasury.
/// Topics: (Symbol, Address, Address) — event name, from, treasury
/// Data: amount
pub fn compliance_confiscated(env: &Env, from: &Address, treasury: &Address, amount: i128) {
    env.events().publish(
        (
            Symbol::new(env, "compliance_confiscated"),
            from.clone(),
            treasury.clone(),
        ),
        amount,
    );
}

/// Emitted when the contract is paused.
/// Topics: (Symbol, Address) — event name, admin
pub fn paused(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "paused"), admin.clone()), ());
}

/// Emitted when the contract is unpaused.
/// Topics: (Symbol, Address) — event name, admin
pub fn unpaused(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "unpaused"), admin.clone()), ());
}

/// Emitted when the contract is upgraded.
/// Topics: (Symbol, Address) — event name, admin
pub fn upgraded(env: &Env, admin: &Address, new_wasm_hash: &soroban_sdk::BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "upgraded"), admin.clone()),
        new_wasm_hash.clone(),
    );
}

/// Emitted when the transfer hook address is changed.
/// Topics: (Symbol, Address) — event name, admin
pub fn transfer_hook_set(env: &Env, admin: &Address, hook: Option<&Address>) {
    env.events()
        .publish((Symbol::new(env, "hook_set"), admin.clone()), hook.cloned());
}

/// Emitted when a balance snapshot is recorded (issue #717).
/// Topics: (Symbol, Address, u32) — event name, account, ledger
pub fn snapshot_taken(env: &Env, account: &Address, ledger: u32, balance: i128) {
    env.events().publish(
        (Symbol::new(env, "snapshot"), account.clone(), ledger),
        balance,
    );
}

/// Emitted when an owner registers (or rotates) their permit signing key.
/// Topics: (Symbol, Address) — event name, owner
pub fn permit_signer_set(env: &Env, owner: &Address) {
    env.events()
        .publish((Symbol::new(env, "permit_signer_set"), owner.clone()), ());
}

/// Emitted when a signature-based permit is successfully applied.
/// Topics: (Symbol, Address, Address) — event name, owner, spender
pub fn permit_used(env: &Env, owner: &Address, spender: &Address, amount: i128, nonce: u32) {
    env.events().publish(
        (
            Symbol::new(env, "permit_used"),
            owner.clone(),
            spender.clone(),
        ),
        (amount, nonce),
    );
}

/// Emitted when an owner cancels a permit nonce, invalidating any signature
/// that was signed for it.
/// Topics: (Symbol, Address) — event name, owner
pub fn permit_nonce_cancelled(env: &Env, owner: &Address, nonce: u32) {
    env.events().publish(
        (Symbol::new(env, "permit_nonce_cancelled"), owner.clone()),
        nonce,
    );
}
