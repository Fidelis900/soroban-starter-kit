use soroban_sdk::{Address, Env, Symbol};

/// Emitted when the escrow is initialized.
/// Topics: (Symbol, Address, Address, Address) — event name, buyer, seller, arbiter
pub fn initialized(env: &Env, buyer: &Address, seller: &Address, arbiter: &Address, amount: i128) {
    env.events().publish(
        (
            Symbol::new(env, "initialized"),
            buyer.clone(),
            seller.clone(),
            arbiter.clone(),
        ),
        amount,
    );
}

/// Emitted when an escrow is created.
/// Topics: (Symbol, Address, Address) — event name, buyer, seller
pub fn escrow_created(env: &Env, buyer: &Address, seller: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "created"), buyer.clone(), seller.clone()),
        amount,
    );
}

/// Emitted when an escrow is funded.
/// Topics: (Symbol, Address) — event name, buyer
pub fn escrow_funded(env: &Env, buyer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "funded"), buyer.clone()), amount);
}

/// Emitted when delivery is marked.
/// Topics: (Symbol, Address) — event name, seller
pub fn delivery_marked(env: &Env, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "marked_delivered"), seller.clone()), ());
}

/// Emitted when funds are released to the seller.
/// Topics: (Symbol, Address) — event name, seller
/// Data: (net_amount, fee_amount) — `net_amount` is what the seller received;
/// `fee_amount` is what was routed to the treasury (0 if no fee configured).
pub fn funds_released(env: &Env, seller: &Address, net_amount: i128, fee_amount: i128) {
    env.events().publish(
        (Symbol::new(env, "released"), seller.clone()),
        (net_amount, fee_amount),
    );
}

/// Emitted when a partial release is made to the seller.
/// Topics: (Symbol, Address) — event name, seller
/// Data: (net_amount, fee_amount) — see [`funds_released`].
pub fn partial_release(env: &Env, seller: &Address, net_amount: i128, fee_amount: i128) {
    env.events().publish(
        (Symbol::new(env, "released_partial"), seller.clone()),
        (net_amount, fee_amount),
    );
}

pub fn funds_refunded(env: &Env, buyer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "refunded"), buyer.clone()), amount);
}

/// Emitted when the escrow amount is updated.
/// Topics: (Symbol, Address) — event name, buyer
pub fn amount_updated(env: &Env, buyer: &Address, new_amount: i128) {
    env.events().publish(
        (Symbol::new(env, "amount_updated"), buyer.clone()),
        new_amount,
    );
}

/// Emitted when the escrow is cancelled.
/// Topics: (Symbol, Address) — event name, buyer
pub fn escrow_cancelled(env: &Env, buyer: &Address) {
    env.events()
        .publish((Symbol::new(env, "escrow_cancelled"), buyer.clone()), ());
}

/// Emitted when the deadline is extended.
/// Topics: (Symbol, Address) — event name, buyer
pub fn deadline_extended(env: &Env, buyer: &Address, new_deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "deadline_extended"), buyer.clone()),
        new_deadline,
    );
}

/// Emitted when a dispute is raised.
/// Topics: (Symbol, Address) — event name, caller
pub fn dispute_raised(env: &Env, caller: &Address) {
    env.events()
        .publish((Symbol::new(env, "dispute_raised"), caller.clone()), ());
}

/// Emitted when a buyer claims a refund after dispute timeout (#709).
/// Topics: (Symbol, Address) — event name, buyer
pub fn dispute_timeout_claimed(env: &Env, buyer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "dispute_timeout"), buyer.clone()), amount);
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

/// Emitted when a milestone's funds are released to the seller.
/// Topics: (Symbol, Address, u32) — event name, seller, milestone_index
/// Data: (amount, fee_amount)
pub fn milestone_released(env: &Env, seller: &Address, index: u32, amount: i128, fee: i128) {
    env.events().publish(
        (
            Symbol::new(env, "milestone_released"),
            seller.clone(),
            index,
        ),
        (amount, fee),
    );
}

/// Emitted when the fee configuration is updated by the admin.
/// Topics: (Symbol, Address) — event name, admin
/// Data: (fee_bps, treasury)
pub fn fee_config_set(env: &Env, admin: &Address, fee_bps: u32, treasury: &Address) {
    env.events().publish(
        (Symbol::new(env, "fee_config_set"), admin.clone()),
        (fee_bps, treasury.clone()),
    );
}

/// Emitted when an arbiter fee is paid out of disputed funds (#1288).
/// Topics: (Symbol, Address) — event name, arbiter
/// Data: fee_amount — the amount transferred to the arbiter.
pub fn arbiter_fee_paid(env: &Env, arbiter: &Address, fee_amount: i128) {
    env.events().publish(
        (Symbol::new(env, "arbiter_fee_paid"), arbiter.clone()),
        fee_amount,
    );
}

/// Emitted when buyer and seller mutually agree to cancel (#1289).
/// Topics: (Symbol, Address, Address) — event name, buyer, seller
/// Data: refund_amount — funds returned to the depositor.
pub fn mutual_cancelled(env: &Env, buyer: &Address, seller: &Address, refund_amount: i128) {
    env.events().publish(
        (
            Symbol::new(env, "mutual_cancelled"),
            buyer.clone(),
            seller.clone(),
        ),
        refund_amount,
    );
}

/// Emitted when a milestone is partially released and the remainder refunded (#1290).
/// Topics: (Symbol, Address, Address, u32) — event name, seller, buyer, milestone_index
/// Data: (released_amount, refunded_amount)
pub fn milestone_partial_released(
    env: &Env,
    seller: &Address,
    buyer: &Address,
    index: u32,
    released_amount: i128,
    refunded_amount: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "milestone_partial"),
            seller.clone(),
            buyer.clone(),
            index,
        ),
        (released_amount, refunded_amount),
    );
}

/// Emitted when an arbiter approves a dispute resolution direction (#1291).
/// Topics: (Symbol, Address) — event name, arbiter
/// Data: direction — true releases to seller, false refunds to buyer.
pub fn arbiter_approved(env: &Env, arbiter: &Address, direction: bool) {
    env.events().publish(
        (Symbol::new(env, "arbiter_approved"), arbiter.clone()),
        direction,
    );
}

/// Emitted when a dispute is resolved once the arbiter threshold is met (#1291).
/// Topics: (Symbol, Address) — event name, arbiter
/// Data: release_to_seller — true if funds released to seller, false if refunded to buyer.
pub fn dispute_resolved(env: &Env, arbiter: &Address, release_to_seller: bool) {
    env.events().publish(
        (Symbol::new(env, "dispute_resolved"), arbiter.clone()),
        release_to_seller,
    );
}
