//! Dutch (descending price) auction pricing (issue #1071).
//!
//! ```text
//! price(t) = start_price - (start_price - floor_price) * (t - start_ledger) / duration_ledgers
//! ```
//!
//! The price equals `start_price` at or before `start_ledger`, falls linearly,
//! and is clamped to `floor_price` from `start_ledger + duration_ledgers` onwards.

use crate::{AuctionError, DutchConfig};
use soroban_common::math::mul_div;

/// Return the Dutch auction price at `ledger` for the schedule `cfg`.
///
/// The linear interpolation `spread * elapsed / duration` is computed with the
/// shared checked fixed-point primitive [`soroban_common::math::mul_div`], which
/// rounds down and surfaces overflow as an error instead of panicking. The
/// result is identical to the floor of the textbook formula.
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

    let decay = mul_div(spread, elapsed, duration).map_err(|_| AuctionError::Overflow)?;

    let price = cfg
        .start_price
        .checked_sub(decay)
        .ok_or(AuctionError::Overflow)?;
    Ok(price.max(cfg.floor_price))
}
