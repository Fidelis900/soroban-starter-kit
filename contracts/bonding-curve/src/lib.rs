Looking at the current file, I need to understand the structure. The file has some duplicated/conflicting content (two `#![no_std]`, two `initialize`/`buy`/`sell` impls). I'll implement the requested features while preserving existing functionality. Since I can't see `storage.rs`, `errors.rs`, or `events.rs`, I'll need to reference the required `DataKey` variants, error variants, and event functions. I'll add them assuming they need to exist — but since I can only edit this file, I'll reference them and note they must exist. However, the task says return complete file content only.

Given the constraints, I'll implement using the referenced items (`DataKey::MaxBuyBps`, `DataKey::MaxSurgePct`, `DataKey::SurgeWindowLedgers`, `DataKey::PriceReference`, `BondingCurveError::PurchaseExceedsLimit`, `BondingCurveError::PriceSurgeDetected`, `events::max_buy_bps_set`, `events::price_surge_detected`). These must be added to the sibling modules, but I can only return this file. I'll implement the logic here referencing them.

Let me write the complete file, consolidating the duplicate impls into one coherent version (keeping the more complete second impl which has fee/graduation logic, and merging in the getters from the first impl).

```rust
#![no_std]
#![deny(missing_docs)]
//! Bonding-curve token sale contract template.
//!
//! Token price scales deterministically with supply along a bonding curve;
//! buyers mint tokens by paying the curve price and sellers burn to redeem.

#[cfg(any(test, feature = "testutils"))]
extern crate std;

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod prop_test;

pub use errors::BondingCurveError;
pub use storage::{CurveState, DataKey, PRICE_SCALE};
use storage::{BPS_DENOMINATOR, MAX_FEE_BPS, MIN_CONNECTOR_WEIGHT_BPS};

use soroban_common::math::{FixedPoint, FixedPointError, apply_bps_fee};
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};
use storage::{BUY_COOLDOWN_LEDGERS, CurveState, MAX_BUYS_PER_LEDGER};
```

Wait — there's `pub use storage::{CurveState, DataKey, PRICE_SCALE};` and then `use storage::{BUY_COOLDOWN_LEDGERS, CurveState, MAX_BUYS_PER_LEDGER};` — duplicate CurveState import. That's in the original. I'll keep it as-is to preserve, though it might be a compile issue. Actually duplicate imports of the same item are allowed in Rust (re-importing same name from same path is fine? No — `use storage::CurveState` twice would be a conflict E0252). But the original has it, so presumably it compiles... Actually `pub use` and `use` of the same item — that's a re-import conflict. Hmm. But it's the existing content. I'll leave it to preserve existing behavior; not my concern for this change.

Actually wait, let me reconsider. The original file has `pub use storage::{CurveState, DataKey, PRICE_SCALE};` then later `use storage::{BUY_COOLDOWN_LEDGERS, CurveState, MAX_BUYS_PER_LEDGER};`. This would be a duplicate import error. But since it's the given content, maybe it does compile in their setup or maybe it's a latent bug. I'll preserve it as-is since I shouldn't change unrelated things.

Now let me write the full file. I'll keep both impl blocks as in original but add the new functions. Actually the original has two `#[contractimpl]` blocks — the first with `initialize(admin, token)` and `buy/sell` (old signatures), the second with the full `initialize(...)` and `buy/sell`. Having two `initialize` with different signatures in two impl blocks — that's method overloading which Rust doesn't allow. So this file as given likely doesn't compile. But again, I preserve.

Hmm, this is tricky. The task says "Preserve all existing functionality not related to the change." I'll preserve the structure but add my new functions. Let me add the new admin setters and modify `buy()` in the second impl block (the full one), and add the getters.

Let me now write it out carefully. I'll add to the second `#[contractimpl]` block:
- `set_max_buy_bps`
- `set_max_surge_pct`
- `set_surge_window_ledgers`
- modify `buy()` to add cap check and surge check
- add getters `get_max_buy_bps`, `get_max_surge_pct`, `get_surge_window_ledgers`

For the cap check: `max_buy_limit = total_max_supply * max_buy_bps / 10_000`. Total max supply = graduation supply cap. If no cap configured, we can't compute — skip check (or use supply? The step says "using the configured graduation supply cap / total max supply"). If cap is None, skip enforcement.

For surge: read `DataKey::PriceReference` storing `(i128, u32)`. If `ref_ledger` within window and current price > ref_price * (10000 + max_surge_pct)/10000, return error.

Let me write the complete file now.

For the surge check, I need current spot price. Use `calculate_price(reserve, supply, base_price, slope)` or stored `DataKey::Price`. The step says "current spot price". I'll compute from stored price or calculate. I'll use the stored `DataKey::Price` as current spot price (it's updated on each buy/sell). Actually to be safe, compute `calculate_price(reserve, supply, base_price, slope)`.

Let me structure the buy() additions:

After cooldown check, before token transfer:

```rust
// ── Max-buy cap guard (#1308) ─────────────────────────────────────
if let Some(max_buy_bps) = env.storage().instance().get::<_, u32>(&DataKey::MaxBuyBps) {
    if let Some(total_max_supply) = env.storage().instance().get::<_, i128>(&DataKey::GraduationSupplyCap) {
        let max_buy_limit = total_max_supply
            .checked_mul(max_buy_bps as i128)
            .ok_or(BondingCurveError::Overflow)?
            / BPS_DENOMINATOR as i128;
        if amount > max_buy_limit {
            return Err(BondingCurveError::PurchaseExceedsLimit);
        }
    }
}
```

Wait, but I need reserve/supply/base_price/slope for surge check. Those are read later. Let me reorder: read reserve/supply/slope/base_price first, then do surge check, then cap check, then cost.

Actually the original reads reserve/supply after the cooldown check. Let me insert surge check after reading those values.

Let me restructure buy():
1. NotInitialized check
2. Graduation guard
3. amount <= 0 check
4. require_auth
5. rate-limit guard
6. cooldown guard
7. read token, reserve, supply, slope, base_price, connector_weight_bps, fee_bps, treasury
8. **surge check** (compute current price, compare to reference)
9. **max-buy cap check**
10. compute cost, fee, etc.
11. transfer
12. update state
13. **update price reference** to new price + current ledger
14. rate-limit state update
15. graduation check
16. events

For surge check:
```rust
let current_price = calculate_price(reserve, supply, base_price, slope)?;
if let Some(max_surge_pct) = env.storage().instance().get::<_, u32>(&DataKey::MaxSurgePct) {
    let window: u32 = env.storage().instance().get(&DataKey::SurgeWindowLedgers).unwrap_or(0);
    if let Some((ref_price, ref_ledger)) = env.storage().instance().get::<_, (i128, u32)>(&DataKey::PriceReference) {
        if ref_price > 0 && current_ledger.saturating_sub(ref_ledger) <= window {
            let threshold = ref_price
                .checked_mul((BPS_DENOMINATOR as i128).checked_add(max_surge_pct as i128).ok_or(BondingCurveError::Overflow)?)
                .ok_or(BondingCurveError::Overflow)?
                / BPS_DENOMINATOR as i128;
            if current_price > threshold {
                events::price_surge_detected(&env, ref_price, current_price, max_surge_pct);
                return Err(BondingCurveError::PriceSurgeDetected);
            }
        }
    }
}
```

On successful buy, update reference:
```rust
env.storage().instance().set(&DataKey::PriceReference, &(new_price, current_ledger));
```

Now the setters:

```rust
pub fn set_max_buy_bps(env: Env, admin: Address, bps: Option<u32>) -> Result<(), BondingCurveError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(BondingCurveError::NotInitialized)?;
    if admin != stored_admin { return Err(BondingCurveError::Unauthorized); }
    admin.require_auth();
    if let Some(b) = bps {
        if b > BPS_DENOMINATOR { return Err(BondingCurveError::InvalidConfiguration); }
        env.storage().instance().set(&DataKey::MaxBuyBps, &b);
    } else {
        env.storage().instance().remove(&DataKey::MaxBuyBps);
    }
    events::max_buy_bps_set(&env, bps);
    bump(&env);
    Ok(())
}
```

Wait, `BPS_DENOMINATOR` is likely `u32` (10000). `b > BPS_DENOMINATOR` — if BPS_DENOMINATOR is u32, fine. Let me use `10_000u32` to be safe, or `BPS_DENOMINATOR`. The step says "validates `bps <= 10_000`". I'll use `BPS_DENOMINATOR`.

Actually `BPS_DENOMINATOR` is used as `BPS_DENOMINATOR as u32` elsewhere (`let exponent = BPS_DENOMINATOR as u32 / connector_weight_bps;`), suggesting it's i128. So `b > BPS_DENOMINATOR` where b is u32 would be type mismatch. I'll use `10_000u32` explicitly.

Similarly for surge setters.

```rust
pub fn set_max_surge_pct(env: Env, admin: Address, max_surge_pct: Option<u32>) -> Result<(), BondingCurveError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(BondingCurveError::NotInitialized)?;
    if admin != stored_admin { return Err(BondingCurveError::Unauthorized); }
    admin.require_auth();
    if let Some(p) = max_surge_pct {
        env.storage().instance().set(&DataKey::MaxSurgePct, &p);
    } else {
        env.storage().instance().remove(&DataKey::MaxSurgePct);
    }
    bump(&env);
    Ok(())
}

pub fn set_surge_window_ledgers(env: Env, admin: Address, window: u32) -> Result<(), BondingCurveError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(BondingCurveError::NotInitialized)?;
    if admin != stored_admin { return Err(BondingCurveError::Unauthorized); }
    admin.require_auth();
    env.storage().instance().set(&DataKey::SurgeWindowLedgers, &window);
    bump(&env);
    Ok(())
}
```

Now, the `events::max_buy_bps_set` and `events::price_surge_detected` — I'll reference them. If they don't exist in events.rs, it won't compile, but I can only edit this file. I'll add them as references.

Hmm, but the task says "Return the COMPLETE new file content" for this file only. The sibling modules presumably need updating too but that's outside scope. I'll reference the required items.

Actually, to be safe and self-contained, maybe I should avoid referencing non-existent event functions. But the step explicitly says "emits `max_buy_bps_set`" and "emit `price_surge_detected`". So I'll reference `events::max_buy_bps_set` and `events::price_surge_detected`.

Now let me also add getters for the new config.

Let me now write the complete file. I'll preserve the two impl blocks structure but add new functions to the second (full) impl block, and modify buy() there.

Wait, but there are two `buy` functions (one in each impl block). The first impl block's `buy` is the old one. The second impl block's `buy` is the full one. Having two `buy` in two impl blocks for the same type — that's a duplicate definition error. So this file as given is broken. But I must preserve. Hmm.

Actually, maybe the intent is that the first impl block is legacy and the second overrides... but Rust doesn't allow that. This is clearly a messy file. Given the instruction "Preserve all existing functionality not related to the change," I'll keep both blocks but only modify the second (full) one. The file won't compile either way due to pre-existing issues, but that's not my concern.

Actually, let me reconsider — maybe I should consolidate to make it compile. But that risks removing functionality. The safest is to preserve structure and add changes. I'll do that.

Let me write it out now. I'll be careful to keep everything.

For the tests, I'll add a new `#[cfg(test)] mod cap_and_surge_test` module at the end with the required tests. But tests need the contract to work. Given the file's state, tests may not run, but I'll write them per spec.

Let me write the tests:

```rust
#[cfg(test)]
mod cap_and_surge_test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::token::StellarAssetClient;

    fn setup(env: &Env) -> (BondingCurveContractClient, Address, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let buyer = Address::generate(env);
        let sac_admin = Address::generate(env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token = sac.address();
        StellarAssetClient::new(env, &token).mint(&buyer, &1_000_000_000i128);
        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(env, &contract_addr);
        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin, &Some(1_000_000i128));
        (contract, admin, buyer)
    }

    #[test]
    fn test_cap_unset_allows_large_buy() {
        let env = Env::default();
        let (contract, _admin, buyer) = setup(&env);
        // No cap set — large buy allowed.
        contract.buy(&buyer, &500_000i128, &i128::MAX);
    }

    #[test]
    fn test_cap_at_limit_succeeds() {
        let env = Env::default();
        let (contract, admin, buyer) = setup(&env);
        contract.set_max_buy_bps(&admin, &Some(100u32)); // 1% of 1_000_000 = 10_000
        contract.buy(&buyer, &10_000i128, &i128::MAX);
    }

    #[test]
    fn test_cap_over_limit_rejected() {
        let env = Env::default();
        let (contract, admin, buyer) = setup(&env);
        contract.set_max_buy_bps(&admin, &Some(100u32));
        let res = contract.try_buy(&buyer, &10_001i128, &i128::MAX);
        assert_eq!(res, Err(Ok(BondingCurveError::PurchaseExceedsLimit)));
    }

    #[test]
    fn test_cap_boundary_bps() {
        let env = Env::default();
        let (contract, admin, _buyer) = setup(&env);
        // 10_000 bps = 100% allowed
        assert!(contract.try_set_max_buy_bps(&admin, &Some(10_000u32)).is_ok());
        // > 10_000 rejected
        assert_eq!(contract.try_set_max_buy_bps(&admin, &Some(10_001u32)), Err(Ok(BondingCurveError::InvalidConfiguration)));
    }

    #[test]
    fn test_surge_breaker_triggers_and_cools_down() {
        let env = Env::default();
        let (contract, admin, buyer) = setup(&env);
        contract.set_max_surge_pct(&admin, &Some(100u32)); // +100%
        contract.set_surge_window_ledgers(&admin, &20u32);

        // First buy sets reference.
        contract.buy(&buyer, &1_000i128, &i128::MAX);

        // Force a large price jump by buying a lot in same window.
        // ... depends on curve; use a large amount.
        // This is heuristic; may need tuning.
        let res = contract.try_buy(&buyer, &500_000i128, &i128::MAX);
        // Expect surge detected (if price doubled).
        // ...
    }
}
```

The surge test is hard to make deterministic without knowing curve params. I'll write it but it may not be reliable. Given constraints, I'll write reasonable tests.

Actually, let me make the surge test more controllable: use a steep slope so a small buy doubles price. With base_price=1, slope=1_000_000, buying 1 token adds ~1_000_000/PRICE_SCALE... hmm PRICE_SCALE unknown. This is getting complicated.

Given I can't see PRICE_SCALE or the exact math, I'll write tests that are structurally correct and rely on the contract's behavior. I'll make the surge test buy a large amount and check for either success or surge error, then advance ledger and verify buys work again.

Let me simplify: I'll write the tests to be as robust as possible.

Actually, for the surge test, I can directly manipulate storage? No, tests use client. 

Let me think about a deterministic surge: The price reference is set on first buy. If I set max_surge_pct very low (e.g., 1 bps = 0.01%), then almost any price increase triggers. So:
- set_max_surge_pct(Some(1)) — 0.01% surge threshold
- set_surge_window_ledgers(20)
- buy small amount → sets reference at price P0
- buy again → price increases → likely > 0.01% → surge detected

But the first buy also needs to not trigger (no reference yet). Good.

Then advance ledger by 21, buy again → window elapsed → allowed.

For sells working while halted: after surge detected, do a sell — should succeed.

Let me write:

```rust
#[test]
fn test_surge_breaker_triggers() {
    let env = Env::default();
    let (contract, admin, buyer) = setup(&env);
    contract.set_max_surge_pct(&admin, &Some(1u32)); // 0.01%
    contract.set_surge_window_ledgers(&admin, &20u32);

    contract.buy(&buyer, &1_000i128, &i128::MAX); // sets reference
    // Next buy raises price beyond 0.01% → surge
    let res = contract.try_buy(&buyer, &1_000i128, &i128::MAX);
    assert_eq!(res, Err(Ok(BondingCurveError::PriceSurgeDetected)));
}

#[test]
fn test_surge_cool_down_allows_buys() {
    let env = Env::default();
    let (contract, admin, buyer) = setup(&env);
    contract.set_max_surge_pct(&admin, &Some(1u32));
    contract.set_surge_window_ledgers(&admin, &20u32);

    contract.buy(&buyer, &1_000i128, &i128::MAX);
    let res = contract.try_buy(&buyer, &1_000i128, &i128::MAX);
    assert_eq!(res, Err(Ok(BondingCurveError::PriceSurgeDetected)));

    // Advance ledger beyond window.
    env.ledger().with_mut(|le| { le.sequence_number += 21; });
    // Now buy should succeed (reference is stale).
    contract.buy(&buyer, &1_000i128, &i128::MAX);
}

#[test]
fn test_sells_work_while_buys_halted() {
    let env = Env::default();
    let (contract, admin, buyer) = setup(&env);
    contract.set_max_surge_pct(&admin, &Some(1u32));
    contract.set_surge_window_ledgers(&admin, &20u32);

    contract.buy(&buyer, &1_000i128, &i128::MAX);
    let res = contract.try_buy(&buyer, &1_000i128, &i128::MAX);
    assert_eq!(res, Err(Ok(BondingCurveError::PriceSurgeDetected)));

    // Sell should still work.
    contract.sell(&buyer, &500i128, &0i128);
}
```

But wait — the cooldown guard: `BUY_COOLDOWN_LEDGERS` between buys from same address. If BUY_COOLDOWN_LEDGERS > 0, the second buy would fail with CooldownActive before reaching surge check. Hmm. The step says surge check is "after existing rate-limit/cooldown checks". So if cooldown blocks, we never reach surge. For tests to trigger surge, I'd need to advance ledger past cooldown but within surge window. If BUY_COOLDOWN_LEDGERS is small (e.g., 1) and surge window is 20, I can advance 1 ledger. But I don't know BUY_COOLDOWN_LEDGERS value.

This is getting too deep. Given I can't see the constants, I'll write tests that advance the ledger appropriately and use different buyers to avoid cooldown. Actually different buyers avoid per-address cooldown! So use buyer1 for first buy, buyer2 for second buy. That avoids cooldown entirely.

Let me use two buyers:

```rust
let buyer2 = Address::generate(&env);
StellarAssetClient::new(&env, &token).mint(&buyer2, &1_000_000_000i128);
```

But setup returns only one buyer. Let me have setup mint to multiple or mint in test. I'll mint in test.

OK this is getting long. Let me just write reasonable tests and move on. The key deliverable is the contract logic.

Let me finalize. I'll write the complete file now.

One more consideration: `env.storage().instance().remove(&DataKey::MaxBuyBps)` — `remove` exists on storage. Good.

For `DataKey::PriceReference` storing `(i128, u32)` — Soroban supports tuples as storage values? Yes, tuples implement `IntoVal`/`TryFromVal` if components do. i128 and u32 do. Good.

Let me write the file.

Also need to handle: the surge check reads `current_ledger` which is defined earlier in buy(). Good.

Let me also make sure `BPS_DENOMINATOR` usage: it's used as `BPS_DENOMINATOR as u32` and `BPS_DENOMINATOR as i128`? In original: `let exponent = BPS_DENOMINATOR as u32 / connector_weight_bps;` and `let root = BPS_DENOMINATOR as u32 / connector_weight_bps;`. So BPS_DENOMINATOR is cast to u32. It could be i128 or u32. I'll use `BPS_DENOMINATOR as i128` for the math to be safe, and `10_000u32` for validation.

Actually for `max_buy_limit = total_max_supply * max_buy_bps / 10_000`, I'll use `BPS_DENOMINATOR as i128`.

Let me write.

For the threshold: `ref_price * (10_000 + max_surge_pct) / 10_000`. I'll compute `(BPS_DENOMINATOR as i128 + max_surge_pct as i128)`.

Now writing the complete file.

I need to be careful with the two impl blocks. Let me keep the first impl block (old initialize/buy/sell/getters) and the second impl block (full). I'll add new functions to the second block and modify its buy(). I'll also add getters to the second block.

Actually, having duplicate `initialize`, `buy`, `sell` across two impl blocks is a compile error. But I preserve. Hmm, but if I add `set_max_buy_bps` etc. to the second block, that's fine (unique names).

Wait, actually maybe the first impl block is meant to be removed/legacy. But instruction says preserve. I'll keep both.

Let me write it all out now. This is going to be long.

Actually, let me reconsider the duplicate issue. The file literally has:
- Block 1: `initialize(env, admin, token)`, `buy(env, buyer, amount, max_cost)`, `sell(...)`, `get_reserve`, `get_supply`, `get_price`, `balance`
- Block 2: `initialize(env, admin, token, initial_slope, base_price, ...)`, `set_fee_bps`, `set_treasury`, `buy(...)`, `sell(...)`, `get_slope`, `get_base_price`, `get_connector_weight_bps`, `get_fee_bps`, `get_treasury`, `get_curve_state`, `get_graduation_supply_cap`, `graduate`, `migrate_to_amm`

Two `initialize` with different arities → error. Two `buy` → error. Two `sell` → error. So this file is fundamentally broken as given. 

Given the task is to implement features, and the file is broken, perhaps the intent is that I should produce a working file. But "preserve existing functionality" — the safest interpretation is to keep the second (more complete) impl block and remove the first (legacy) one, since they conflict. But that removes `get_reserve`, `get_supply`, `get_price`, `balance` which are in block 1.

Hmm. Let me merge: keep block 2's functions and add block 1's getters (`get_reserve`, `get_supply`, `get_price`, `balance`) to block 2. Remove block 1's `initialize`, `buy`, `sell` (superseded by block 2). This produces a coherent file.

But that's a big change. The instruction says "Preserve all existing functionality not related to the change." The getters are functionality. The old initialize/buy/sell are superseded by the new ones (same names). So merging is reasonable.

I'll do the merge: single impl block with all functions, using block 2's versions of initialize/buy/sell, plus block 1's getters, plus new functions.

This is the cleanest. Let me do that.

Final structure:
- One `#[contractimpl] impl BondingCurveContract { ... }` with:
  - initialize (full signature)
  - set_fee_bps, set_treasury
  - set_max_buy_bps, set_max_surge_pct, set_surge_window_ledgers (NEW)
  - buy (modified with cap + surge)
  - sell (unchanged)
  - get_reserve, get_supply, get_price, balance (from block 1)
  - get_slope, get_base_price, get_connector_weight_bps, get_fee_bps, get_treasury
  - get_max_buy_bps, get_max_surge_pct, get_surge_window_ledgers (NEW)
  - get_curve_state, get_graduation_supply_cap, graduate, migrate_to_amm

Good. Let me write.

Also fix the duplicate `#![no_std]` — keep one.

And the duplicate `use storage::CurveState` — I'll keep `pub use storage::{CurveState, DataKey, PRICE_SCALE};` and change the second to `use storage::{BUY_COOLDOWN_LEDGERS, MAX_BUYS_PER_LEDGER};` (remove duplicate CurveState). That's a minor fix to avoid conflict.

Let me write the complete file.

For events, I'll reference `events::max_buy_bps_set(&env, bps)` and `events::price_surge_detected(&env, ref_price, current_price, max_surge_pct)`. These need to exist in events.rs. I'll assume they do or will be added.

Now writing.

Let me carefully write the buy() function with all guards in order:

```rust
pub fn buy(env: Env, buyer: Address, amount: i128, max_cost: i128) -> Result<(), BondingCurveError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(BondingCurveError::NotInitialized);
    }

    let state: CurveState = env.storage().instance().get(&DataKey::CurveState).unwrap_or(CurveState::Active);
    if state == CurveState::Graduated {
        return Err(BondingCurveError::CurveGraduated);
    }

    if amount <= 0 {
        return Err(BondingCurveError::InvalidAmount);
    }
    buyer.require_auth();

    let current_ledger = env.ledger().sequence();

    // rate-limit
    let last_ledger_key = DataKey::LastBuyLedger(buyer.clone());
    let buys_key = DataKey::BuysThisLedger(buyer.clone());
    let last_buy_ledger: u32 = env.storage().persistent().get(&last_ledger_key).unwrap_or(0);
    let buys_this_ledger: u32 = if last_buy_ledger == current_ledger {
        env.storage().persistent().get(&buys_key).unwrap_or(0)
    } else { 0 };
    if buys_this_ledger >= MAX_BUYS_PER_LEDGER {
        return Err(BondingCurveError::RateLimitExceeded);
    }

    // cooldown
    if last_buy_ledger > 0 {
        let ledgers_since_last = current_ledger.saturating_sub(last_buy_ledger);
        if ledgers_since_last < BUY_COOLDOWN_LEDGERS {
            return Err(BondingCurveError::CooldownActive);
        }
    }

    let token: Address = env.storage().instance().get(&DataKey::Token).ok_or(BondingCurveError::NotInitialized)?;
    let reserve: i128 = env.storage().instance().get(&DataKey::Reserve).unwrap_or(0i128);
    let supply: i128 = env.storage().instance().get(&DataKey::Supply).unwrap_or(0i128);
    let slope: i128 = env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128);
    let base_price: i128 = env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128);
    let connector_weight_bps: u32 = env.storage().instance().get(&DataKey::ConnectorWeightBps).unwrap_or(10_000u32);
    let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32);
    let treasury: Address = env.storage().instance().get(&DataKey::Treasury).ok_or(BondingCurveError::NotInitialized)?;

    // ── Circuit breaker (#1309) ──────────────────────────────────────
    let current_price = calculate_price(reserve, supply, base_price, slope)?;
    if let Some(max_surge_pct) = env.storage().instance().get::<_, u32>(&DataKey::MaxSurgePct) {
        let window: u32 = env.storage().instance().get(&DataKey::SurgeWindowLedgers).unwrap_or(0);
        if let Some((ref_price, ref_ledger)) = env.storage().instance().get::<_, (i128, u32)>(&DataKey::PriceReference) {
            if ref_price > 0 && current_ledger.saturating_sub(ref_ledger) <= window {
                let multiplier = (BPS_DENOMINATOR as i128)
                    .checked_add(max_surge_pct as i128)
                    .ok_or(BondingCurveError::Overflow)?;
                let threshold = ref_price
                    .checked_mul(multiplier)
                    .ok_or(BondingCurveError::Overflow)?
                    / (BPS_DENOMINATOR as i128);
                if current_price > threshold {
                    events::price_surge_detected(&env, ref_price, current_price, max_surge_pct);
                    return Err(BondingCurveError::PriceSurgeDetected);
                }
            }
        }
    }

    // ── Max-buy cap (#1308) ──────────────────────────────────────────
    if let Some(max_buy_bps) = env.storage().instance().get::<_, u32>(&DataKey::MaxBuyBps) {
        if let Some(total_max_supply) = env.storage().instance().get::<_, i128>(&DataKey::GraduationSupplyCap) {
            let max_buy_limit = total_max_supply
                .checked_mul(max_buy_bps as i128)
                .ok_or(BondingCurveError::Overflow)?
                / (BPS_DENOMINATOR as i128);
            if amount > max_buy_limit {
                return Err(BondingCurveError::PurchaseExceedsLimit);
            }
        }
    }

    let cost = buy_cost(reserve, supply, amount, base_price, slope, connector_weight_bps)?;
    let fee = apply_bps_fee(cost, fee_bps)?;
    let reserve_credit = cost.checked_sub(fee).ok_or(BondingCurveError::Overflow)?;
    if cost > max_cost || reserve_credit <= 0 {
        return Err(BondingCurveError::InvalidAmount);
    }

    token::Client::new(&env, &token).transfer(&buyer, &env.current_contract_address(), &cost);
    if fee > 0 {
        token::Client::new(&env, &token).transfer(&env.current_contract_address(), &treasury, &fee);
    }

    let new_supply = supply.checked_add(amount).ok_or(BondingCurveError::Overflow)?;
    let new_reserve = reserve.checked_add(reserve_credit).ok_or(BondingCurveError::Overflow)?;
    let new_price = calculate_price(new_reserve, new_supply, base_price, slope)?;

    env.storage().instance().set(&DataKey::Supply, &new_supply);
    env.storage().instance().set(&DataKey::