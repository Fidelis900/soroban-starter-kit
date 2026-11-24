use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, ticket_price: i128) {
    env.events().publish(
        (Symbol::new(env, "initialized"), admin.clone()),
        ticket_price,
    );
}

pub fn ticket_purchased(env: &Env, buyer: &Address) {
    env.events()
        .publish((Symbol::new(env, "ticket_purchased"), buyer.clone()), ());
}

pub fn tickets_batch_purchased(env: &Env, buyer: &Address, count: u32) {
    env.events().publish(
        (Symbol::new(env, "tickets_batch_purchased"), buyer.clone()),
        count,
    );
}

pub fn committed(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "committed"), admin.clone()), ());
}

pub fn winner_drawn(env: &Env, winner: &Address, prize: i128) {
    env.events()
        .publish((Symbol::new(env, "winner_drawn"), winner.clone()), prize);
}

pub fn fee_transferred(env: &Env, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "fee_transferred"), recipient.clone()),
        amount,
    );
}

/// Emitted when the community treasury share is transferred at draw time.
pub fn treasury_paid(env: &Env, treasury: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "treasury_paid"), treasury.clone()),
        amount,
    );
}

pub fn refund_claimed(env: &Env, buyer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "refund_claimed"), buyer.clone()), amount);
}

pub fn rolled_over(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "rolled_over"), admin.clone()), ());
}

/// Emitted on a no-winner rollover, carrying the accumulated balance forward.
///
/// `carried_balance` is the total prize pool (including any previously rolled
/// over amounts) that will seed the next round's jackpot.
pub fn jackpot_rollover(env: &Env, carried_balance: i128) {
    env.events()
        .publish((Symbol::new(env, "jackpot_rollover"),), carried_balance);
}
