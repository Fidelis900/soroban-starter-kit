//! Dutch (descending price) auction pricing (issue #1071).
//!
//! ```text
//! price(t) = start_price - (start_price - floor_price) * (t - start_ledger) / duration_ledgers
//! ```
//!
//! The price equals `start_price` at or before `start_ledger`, falls linearly,
//! and is clamped to `floor_price` from `start_ledger + duration_ledgers` onwards.

use crate::{AuctionError, DutchConfig};

/// Return the Dutch auction price at `ledger` for the schedule `cfg`.
///
/// The product `spread * elapsed` is split as `q * elapsed + r * elapsed / duration`
/// (with `spread = q * duration + r`) so the intermediate values never exceed
/// `spread`, even when `start_price` is close to `i128::MAX`. The result is
/// identical to the floor of the textbook formula.
///
/// # Errors
///
/// - [`AuctionError::Overflow`] if any checked operation overflows (unreachable
///   for schedules accepted by `start_dutch`, which enforces
///   `0 <= floor_price < start_price` and `duration_ledgers > 0`).
pub fn price_at(cfg: &DutchConfig, ledger: u32) -> Result<i128, AuctionError> {
    let elapsed = ledger.saturating_sub(cfg.start_ledger);
    if elapsed >= cfg.duration_ledgers {
        return Ok(cfg.floor_price);
    }

    let spread = cfg
        .start_price
        .checked_sub(cfg.floor_price)
        .ok_or(AuctionError::Overflow)?;
    let elapsed = i128::from(elapsed);
    let duration = i128::from(cfg.duration_ledgers);

    let quotient = spread.checked_div(duration).ok_or(AuctionError::Overflow)?;
    let remainder = spread.checked_rem(duration).ok_or(AuctionError::Overflow)?;
    let remainder_decay = remainder
        .checked_mul(elapsed)
        .and_then(|v| v.checked_div(duration))
        .ok_or(AuctionError::Overflow)?;
    let decay = quotient
        .checked_mul(elapsed)
        .and_then(|v| v.checked_add(remainder_decay))
        .ok_or(AuctionError::Overflow)?;

    let price = cfg
        .start_price
        .checked_sub(decay)
        .ok_or(AuctionError::Overflow)?;
    Ok(price.max(cfg.floor_price))
}
