//! Time and ledger conversion utilities.
//!
//! Soroban ledgers close roughly every [`LEDGER_SECONDS`] seconds. Contracts and
//! off-chain tooling frequently need to convert between ledger counts and
//! wall-clock seconds. These helpers centralize that arithmetic so rounding
//! behavior is consistent everywhere.

/// Number of seconds between consecutive ledgers.
pub const LEDGER_SECONDS: u64 = 5;

/// Converts a number of ledgers into seconds.
///
/// Widening multiplication is used so there is no overflow for any `u32`
/// input (`u32::MAX * 5` fits comfortably in a `u64`).
pub fn ledgers_to_seconds(ledgers: u32) -> u64 {
    (ledgers as u64) * LEDGER_SECONDS
}

/// Converts a number of seconds into ledgers, rounding up.
///
/// Ceiling rounding is used so that a deadline expressed in seconds is never
/// reached before the corresponding number of ledgers has elapsed. This
/// provides a safety buffer for callers relying on ledger-based deadlines.
///
/// The result saturates to [`u32::MAX`] if the conversion would overflow.
pub fn seconds_to_ledgers(seconds: u64) -> u32 {
    if seconds == 0 {
        return 0;
    }

    // Ceiling division: (seconds + LEDGER_SECONDS - 1) / LEDGER_SECONDS.
    // Use checked arithmetic to avoid overflow when `seconds` is near u64::MAX.
    let ledgers = match seconds.checked_add(LEDGER_SECONDS - 1) {
        Some(adjusted) => adjusted / LEDGER_SECONDS,
        // Overflow in the addition means the quotient is certainly larger than
        // u32::MAX, so saturate.
        None => return u32::MAX,
    };

    if ledgers > u32::MAX as u64 {
        u32::MAX
    } else {
        ledgers as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledgers_to_seconds_zero() {
        assert_eq!(ledgers_to_seconds(0), 0);
    }

    #[test]
    fn ledgers_to_seconds_one() {
        assert_eq!(ledgers_to_seconds(1), LEDGER_SECONDS);
    }

    #[test]
    fn ledgers_to_seconds_multiple() {
        assert_eq!(ledgers_to_seconds(10), 50);
        assert_eq!(ledgers_to_seconds(120), 600);
    }

    #[test]
    fn ledgers_to_seconds_max() {
        assert_eq!(ledgers_to_seconds(u32::MAX), (u32::MAX as u64) * LEDGER_SECONDS);
    }

    #[test]
    fn seconds_to_ledgers_zero() {
        assert_eq!(seconds_to_ledgers(0), 0);
    }

    #[test]
    fn seconds_to_ledgers_exact_multiples() {
        assert_eq!(seconds_to_ledgers(LEDGER_SECONDS), 1);
        assert_eq!(seconds_to_ledgers(50), 10);
        assert_eq!(seconds_to_ledgers(600), 120);
    }

    #[test]
    fn seconds_to_ledgers_rounds_up() {
        assert_eq!(seconds_to_ledgers(1), 1);
        assert_eq!(seconds_to_ledgers(4), 1);
        assert_eq!(seconds_to_ledgers(6), 2);
        assert_eq!(seconds_to_ledgers(9), 2);
        assert_eq!(seconds_to_ledgers(11), 3);
    }

    #[test]
    fn seconds_to_ledgers_large_values() {
        // Just below the u32::MAX boundary in ledgers.
        let max_ledgers = u32::MAX as u64;
        let exact_seconds = max_ledgers * LEDGER_SECONDS;
        assert_eq!(seconds_to_ledgers(exact_seconds), u32::MAX);

        // One second past the exact boundary still fits (rounds up to MAX).
        assert_eq!(seconds_to_ledgers(exact_seconds + 1), u32::MAX);

        // Well beyond the boundary saturates.
        assert_eq!(seconds_to_ledgers(exact_seconds + LEDGER_SECONDS), u32::MAX);
    }

    #[test]
    fn seconds_to_ledgers_saturates_on_overflow() {
        assert_eq!(seconds_to_ledgers(u64::MAX), u32::MAX);
        assert_eq!(seconds_to_ledgers(u64::MAX - 1), u32::MAX);
    }

    #[test]
    fn round_trip_ledgers() {
        for ledgers in [0u32, 1, 2, 7, 100, 1_000, u32::MAX] {
            let seconds = ledgers_to_seconds(ledgers);
            assert_eq!(seconds_to_ledgers(seconds), ledgers);
        }
    }

    #[test]
    fn round_trip_seconds_rounds_up() {
        for seconds in [0u64, 1, 4, 5, 6, 9, 10, 11, 100, 123] {
            let ledgers = seconds_to_ledgers(seconds);
            // The converted ledger count must cover at least the original seconds.
            assert!(ledgers_to_seconds(ledgers) >= seconds);
            // And it must not overshoot by a full ledger (except for zero).
            if seconds > 0 {
                assert!(ledgers_to_seconds(ledgers) - seconds < LEDGER_SECONDS);
            }
        }
    }
}