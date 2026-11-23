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

pub fn refund_claimed(env: &Env, buyer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "refund_claimed"), buyer.clone()), amount);
}

pub fn fee_transferred(env: &Env, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "fee_transferred"), recipient.clone()),
        amount,
    );
}
