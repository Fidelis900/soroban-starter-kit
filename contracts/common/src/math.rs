// `#[contracterror]` generates an undocumented associated function; suppress
// the lint for this module only.
#![allow(missing_docs)]
//! Checked fixed-point arithmetic helpers shared across the contracts workspace.
//!
//! This module provides a small, audited set of routines for performing
//! fixed-point multiplication and division with explicit rounding control and
//! overflow detection. All operations use checked integer arithmetic and return
//! a [`Result`] rather than panicking, so callers can decide how to handle
//! arithmetic failures.
//!
//! The primary entry points are the free functions [`mul_div`],
//! [`mul_div_up`], and [`mul_div_nearest`], together with the scale conversion
//! helpers [`to_scale`], [`from_scale`], [`scale_mul`], and [`scale_div`].
//!
//! A [`FixedPoint128`] newtype wrapper is also provided for callers that prefer
//! a typed value carrying a scale factor.

use soroban_sdk::contracterror;

/// Error type returned by fixed-point arithmetic operations.
///
/// All variants are produced instead of panicking so that callers can handle
/// arithmetic failures gracefully.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FixedPointError {
    /// An intermediate or final multiplication/addition overflowed `i128`.
    Overflow = 1,
    /// A division by zero was attempted.
    DivisionByZero = 2,
    /// A scale factor was zero or otherwise invalid.
    InvalidScale = 3,
    /// A negative input was supplied to an operation that requires a
    /// non-negative value (e.g. the integer square root).
    NegativeInput = 4,
}

impl FixedPointError {
    /// Returns a short human-readable description of the error.
    pub fn message(&self) -> &'static str {
        match self {
            FixedPointError::Overflow => "fixed-point arithmetic overflow",
            FixedPointError::DivisionByZero => "fixed-point division by zero",
            FixedPointError::InvalidScale => "invalid fixed-point scale",
            FixedPointError::NegativeInput => "negative input not allowed",
        }
    }
}

impl core::fmt::Display for FixedPointError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message())
    }
}

/// Computes `(a * b) / denominator`, truncating toward zero (floor for
/// non-negative operands).
///
/// # Errors
///
/// Returns [`FixedPointError::DivisionByZero`] if `denominator` is zero, and
/// [`FixedPointError::Overflow`] if the intermediate product or the final
/// result does not fit in an `i128`.
pub fn mul_div(a: i128, b: i128, denominator: i128) -> Result<i128, FixedPointError> {
    if denominator == 0 {
        return Err(FixedPointError::DivisionByZero);
    }
    let product = a.checked_mul(b).ok_or(FixedPointError::Overflow)?;
    product
        .checked_div(denominator)
        .ok_or(FixedPointError::Overflow)
}

/// Computes `(a * b) / denominator`, rounding up (ceiling) toward positive
/// infinity.
///
/// # Errors
///
/// Returns [`FixedPointError::DivisionByZero`] if `denominator` is zero, and
/// [`FixedPointError::Overflow`] if the intermediate product or the final
/// result does not fit in an `i128`.
pub fn mul_div_up(a: i128, b: i128, denominator: i128) -> Result<i128, FixedPointError> {
    if denominator == 0 {
        return Err(FixedPointError::DivisionByZero);
    }
    let product = a.checked_mul(b).ok_or(FixedPointError::Overflow)?;
    let quotient = product
        .checked_div(denominator)
        .ok_or(FixedPointError::Overflow)?;
    let remainder = product
        .checked_rem(denominator)
        .ok_or(FixedPointError::Overflow)?;
    if remainder == 0 {
        return Ok(quotient);
    }
    // Round away from zero when the exact result is not an integer.
    if (remainder > 0) == (denominator > 0) {
        quotient.checked_add(1).ok_or(FixedPointError::Overflow)
    } else {
        Ok(quotient)
    }
}

/// Computes `(a * b) / denominator`, rounding to the nearest integer with ties
/// rounded away from zero (round-half-up in magnitude).
///
/// # Errors
///
/// Returns [`FixedPointError::DivisionByZero`] if `denominator` is zero, and
/// [`FixedPointError::Overflow`] if the intermediate product or the final
/// result does not fit in an `i128`.
pub fn mul_div_nearest(a: i128, b: i128, denominator: i128) -> Result<i128, FixedPointError> {
    if denominator == 0 {
        return Err(FixedPointError::DivisionByZero);
    }
    let product = a.checked_mul(b).ok_or(FixedPointError::Overflow)?;
    let quotient = product
        .checked_div(denominator)
        .ok_or(FixedPointError::Overflow)?;
    let remainder = product
        .checked_rem(denominator)
        .ok_or(FixedPointError::Overflow)?;
    if remainder == 0 {
        return Ok(quotient);
    }
    // Compare 2 * |remainder| against |denominator| to decide rounding.
    let abs_remainder = remainder.checked_abs().ok_or(FixedPointError::Overflow)?;
    let abs_denominator = denominator
        .checked_abs()
        .ok_or(FixedPointError::Overflow)?;
    let twice_remainder = abs_remainder
        .checked_mul(2)
        .ok_or(FixedPointError::Overflow)?;
    if twice_remainder >= abs_denominator {
        // Round away from zero.
        if (remainder > 0) == (denominator > 0) {
            quotient.checked_add(1).ok_or(FixedPointError::Overflow)
        } else {
            quotient.checked_sub(1).ok_or(FixedPointError::Overflow)
        }
    } else {
        Ok(quotient)
    }
}

/// Multiplies `value` by `scale`, i.e. converts a value into a scaled
/// representation.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is zero, and
/// [`FixedPointError::Overflow`] if the product does not fit in an `i128`.
pub fn to_scale(value: i128, scale: i128) -> Result<i128, FixedPointError> {
    if scale == 0 {
        return Err(FixedPointError::InvalidScale);
    }
    value.checked_mul(scale).ok_or(FixedPointError::Overflow)
}

/// Divides `value` by `scale`, i.e. converts a scaled value back to its
/// unscaled representation, truncating toward zero.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is zero, and
/// [`FixedPointError::Overflow`] if the division does not fit in an `i128`.
pub fn from_scale(value: i128, scale: i128) -> Result<i128, FixedPointError> {
    if scale == 0 {
        return Err(FixedPointError::InvalidScale);
    }
    value.checked_div(scale).ok_or(FixedPointError::Overflow)
}

/// Multiplies two scaled values, dividing by `scale` to keep the result in the
/// same scale. Equivalent to [`mul_div`] with `denominator = scale`.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is zero, and
/// [`FixedPointError::Overflow`] on overflow.
pub fn scale_mul(a: i128, b: i128, scale: i128) -> Result<i128, FixedPointError> {
    if scale == 0 {
        return Err(FixedPointError::InvalidScale);
    }
    mul_div(a, b, scale)
}

/// Divides one scaled value by another, multiplying by `scale` to keep the
/// result in the same scale. Equivalent to [`mul_div`] with
/// `denominator = b` and an extra `scale` factor.
///
/// # Errors
///
/// Returns [`FixedPointError::DivisionByZero`] if `b` is zero,
/// [`FixedPointError::InvalidScale`] if `scale` is zero, and
/// [`FixedPointError::Overflow`] on overflow.
pub fn scale_div(a: i128, b: i128, scale: i128) -> Result<i128, FixedPointError> {
    if b == 0 {
        return Err(FixedPointError::DivisionByZero);
    }
    if scale == 0 {
        return Err(FixedPointError::InvalidScale);
    }
    let numerator = a.checked_mul(scale).ok_or(FixedPointError::Overflow)?;
    numerator.checked_div(b).ok_or(FixedPointError::Overflow)
}

/// Computes the integer square root of `value`, i.e. the floor of the exact
/// square root, using a checked binary search over `i128`.
///
/// This is used to derive quadratic voting weights from token balances.
///
/// # Errors
///
/// Returns [`FixedPointError::NegativeInput`] if `value` is negative. The
/// computation never overflows: the search range is bounded by `value` itself
/// and the midpoint is squared with checked arithmetic.
pub fn fixed_sqrt(value: i128) -> Result<i128, FixedPointError> {
    if value < 0 {
        return Err(FixedPointError::NegativeInput);
    }
    if value < 2 {
        return Ok(value);
    }

    // Binary search for the largest `r` such that `r * r <= value`.
    // Invariant: `low * low <= value` and `high * high > value`.
    let mut low: i128 = 1;
    let mut high: i128 = value;
    while low < high {
        // Round the midpoint up so the loop always makes progress.
        let mid = low + (high - low + 1) / 2;
        match mid.checked_mul(mid) {
            Some(square) if square <= value => low = mid,
            // Either the square exceeds `value` or it overflowed `i128`;
            // in both cases the root is strictly below `mid`.
            _ => high = mid - 1,
        }
    }
    Ok(low)
}

/// Computes the fixed-point square root of `val`, returning a result in the
/// same `scale` as the input.
///
/// The input `val` is interpreted as the scaled representation of a
/// non-negative real number, i.e. the true value is `val / scale`. The result
/// `r` satisfies `r / scale ≈ sqrt(val / scale)`, computed via the
/// Babylonian/Newton-Raphson iteration
/// `x_{n+1} = (x_n + (val * scale) / x_n) / 2`, which converges to
/// `sqrt(val * scale)`.
///
/// The iteration starts from a power-of-two estimate derived from the bit
/// length of `val * scale` and stops once the estimate is stable, so the
/// result is the floor of the exact fixed-point square root.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `val` is negative or `scale`
/// is not strictly positive, and [`FixedPointError::Overflow`] if the
/// intermediate `val * scale` product does not fit in an `i128`.
pub fn fixed_sqrt_scaled(val: i128, scale: i128) -> Result<i128, FixedPointError> {
    if val < 0 || scale <= 0 {
        return Err(FixedPointError::InvalidScale);
    }
    if val == 0 {
        return Ok(0);
    }
    // We compute sqrt(val * scale), which is the scaled square root.
    let radicand = val.checked_mul(scale).ok_or(FixedPointError::Overflow)?;

    // Initial guess: the smallest power of two whose square is >= radicand.
    // `bit_length` is at least 1 for radicand >= 1.
    let bit_length = 128 - radicand.leading_zeros() as u32;
    let shift = (bit_length + 1) / 2;
    let mut x: i128 = 1i128 << shift;

    // Newton-Raphson iteration. The sequence decreases monotonically once the
    // initial guess is above the true root, so we stop when it no longer
    // decreases.
    loop {
        let quotient = radicand.checked_div(x).ok_or(FixedPointError::Overflow)?;
        let next = x.checked_add(quotient).ok_or(FixedPointError::Overflow)? / 2;
        if next >= x {
            break;
        }
        x = next;
    }

    Ok(x)
}

/// The canonical scale factor used by the Bancor-style bonding-curve helpers:
/// `10^18`, i.e. 18 decimal places of precision.
pub const BANCOR_SCALE: i128 = 1_000_000_000_000_000_000;

/// Maximum exponent accepted by [`bancor_pow`].
///
/// Exponents above this bound are rejected with
/// [`FixedPointError::Overflow`] because the repeated-squaring loop would
/// otherwise be able to run for an unbounded number of iterations.
pub const BANCOR_MAX_EXPONENT: u32 = 128;

/// Computes `base^exponent` in the fixed-point domain, i.e. returns
/// `(base / scale)^exponent * scale` truncated toward zero.
///
/// This is the core primitive for Bancor-style exponential bonding curves,
/// where the price is `reserve_ratio^(supply / max_supply)` or similar. The
/// computation uses exponentiation by squaring with every intermediate
/// multiplication routed through [`scale_mul`], so all arithmetic is checked
/// and an overflow is reported as [`FixedPointError::Overflow`] rather than
/// panicking.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is not strictly
/// positive, [`FixedPointError::NegativeInput`] if `base` is negative, and
/// [`FixedPointError::Overflow`] if `exponent` exceeds
/// [`BANCOR_MAX_EXPONENT`] or any intermediate product overflows `i128`.
pub fn bancor_pow(base: i128, exponent: u32, scale: i128) -> Result<i128, FixedPointError> {
    if scale <= 0 {
        return Err(FixedPointError::InvalidScale);
    }
    if base < 0 {
        return Err(FixedPointError::NegativeInput);
    }
    if exponent > BANCOR_MAX_EXPONENT {
        return Err(FixedPointError::Overflow);
    }

    // `result` starts at 1.0 in the fixed-point domain.
    let mut result = scale;
    let mut factor = base;
    let mut exp = exponent;
    while exp > 0 {
        if exp & 1 == 1 {
            result = scale_mul(result, factor, scale)?;
        }
        exp >>= 1;
        if exp > 0 {
            factor = scale_mul(factor, factor, scale)?;
        }
    }
    Ok(result)
}

/// Computes the Bancor-style spot price for a given `supply`.
///
/// The curve is parameterised by a `base` price (in `scale`) and an
/// `exponent` applied to the normalised supply. Concretely the price is
/// `base * (1 + supply / scale)^exponent`, evaluated in the fixed-point
/// domain. Because `1 + supply / scale` is non-negative and non-decreasing in
/// `supply`, and `base` is non-negative, the resulting price is guaranteed to
/// be non-decreasing in `supply` — the monotonic pricing invariant.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is not strictly
/// positive, [`FixedPointError::NegativeInput`] if `base` or `supply` is
/// negative, and [`FixedPointError::Overflow`] if the exponent is out of range
/// or any intermediate product overflows `i128`.
pub fn bancor_price(
    base: i128,
    supply: i128,
    exponent: u32,
    scale: i128,
) -> Result<i128, FixedPointError> {
    if scale <= 0 {
        return Err(FixedPointError::InvalidScale);
    }
    if base < 0 || supply < 0 {
        return Err(FixedPointError::NegativeInput);
    }
    // Normalised growth factor: 1.0 + supply (in scale).
    let growth = supply.checked_add(scale).ok_or(FixedPointError::Overflow)?;
    let factor = bancor_pow(growth, exponent, scale)?;
    scale_mul(base, factor, scale)
}

/// Computes the Bancor-style integral (area under the curve) from `0` to
/// `supply`, used to derive the reserve/collateral required to mint `supply`
/// tokens.
///
/// For a curve `price(s) = base * (1 + s)^exponent` the integral is
/// `base * (1 + supply)^(exponent + 1) / (exponent + 1)`. The division is
/// performed with [`mul_div`] so the intermediate product is checked.
///
/// # Errors
///
/// Returns [`FixedPointError::InvalidScale`] if `scale` is not strictly
/// positive, [`FixedPointError::NegativeInput`] if `base` or `supply` is
/// negative, and [`FixedPointError::Overflow`] if the exponent is out of range
/// or any intermediate product overflows `i128`.
pub fn bancor_integral(
    base: i128,
    supply: i128,
    exponent: u32,
    scale: i128,
) -> Result<i128, FixedPointError> {
    if scale <= 0 {
        return Err(FixedPointError::InvalidScale);
    }
    if base < 0 || supply < 0 {
        return Err(FixedPointError::NegativeInput);
    }
    let next_exponent = exponent.checked_add(1).ok_or(FixedPointError::Overflow)?;
    let growth = supply.checked_add(scale).ok_or(FixedPointError::Overflow)?;
    let factor = bancor_pow(growth, next_exponent, scale)?;
    // base * factor / (exponent + 1), all in the fixed-point domain.
    let numerator = scale_mul(base, factor, scale)?;
    let divisor = i128::from(next_exponent);
    mul_div(numerator, scale, divisor)
}

/// A fixed-point value represented as an `i128` mantissa together with an
/// `i128` scale factor.
///
/// The represented value is `mantissa / scale`. All arithmetic is checked and
/// returns [`FixedPointError`] on failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FixedPoint128 {
    /// The raw mantissa, i.e. `value * scale`.
    pub mantissa: i128,
    /// The scale factor; must be non-zero.
    pub scale: i128,
}

impl FixedPoint128 {
    /// Creates a new fixed-point value from a raw `mantissa` and `scale`.
    ///
    /// # Errors
    ///
    /// Returns [`FixedPointError::InvalidScale`] if `scale` is zero.
    pub fn new(mantissa: i128, scale: i128) -> Result<Self, FixedPointError> {
        if scale == 0 {
            return Err(FixedPointError::InvalidScale);
        }
        Ok(Self { mantissa, scale })
    }

    /// Creates a fixed-point value from an unscaled `value` and a `scale`.
    ///
    /// # Errors
    ///
    /// Returns [`FixedPointError::InvalidScale`] if `scale` is zero, and
    /// [`FixedPointError::Overflow`] if the scaling multiplication overflows.
    pub fn from_value(value: i128, scale: i128) -> Result<Self, FixedPointError> {
        let mantissa = to_scale(value, scale)?;
        Ok(Self { mantissa, scale })
    }

    /// Returns the unscaled value, truncating toward zero.
    ///
    /// # Errors
    ///
    /// Returns [`FixedPointError::InvalidScale`] if the scale is zero.
    pub fn to_value(&self) -> Result<i128, FixedPointError> {
        from_scale(self.mantissa, self.scale)
    }

    /// Multiplies this value by another fixed-point value, keeping the scale.
    ///
    /// # Errors
    ///
    /// Returns [`FixedPointError::InvalidScale`] if the scales differ or are
    /// zero, and [`FixedPointError::Overflow`] on overflow.
    pub fn checked_mul(&self, other: &FixedPoint128) -> Result<FixedPoint128, FixedPointError> {
        if self.scale != other.scale {
            return Err(FixedPointError::InvalidScale);
        }
        let mantissa = scale_mul(self.mantissa, other.mantissa, self.scale)?;
        Ok(FixedPoint128 {
            mantissa,
            scale: self.scale,
        })
    }

    /// Divides this value by another fixed-point value, keeping the scale.
    ///
    /// # Errors
    ///
    /// Returns [`FixedPointError::InvalidScale`] if the scales differ or are
    /// zero, [`FixedPointError::DivisionByZero`] if `other` is zero, and
    /// [`FixedPointError::Overflow`] on overflow.
    pub fn checked_div(&self, other: &FixedPoint128) -> Result<FixedPoint128, FixedPointError> {
        if self.scale != other.scale {
            return Err(FixedPointError::InvalidScale);
        }
        let mantissa = scale_div(self.mantissa, other.mantissa, self.scale)?;
        Ok(FixedPoint128 {
            mantissa,
            scale: self.scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// `mul_div` is commutative in its first two arguments.
        #[test]
        fn mul_div_commutative(a in -1_000_000_000i128..1_000_000_000, b in -1_000_000_000i128..1_000_000_000, d in 1i128..1_000_000_000) {
            let ab = mul_div(a, b, d);
            let ba = mul_div(b, a, d);
            prop_assert_eq!(ab, ba);
        }

        /// Rounding bounds: floor <= nearest <= ceil, and each is within one
        /// unit of the exact rational result.
        #[test]
        fn rounding_bounds(a in -1_000_000_000i128..1_000_000_000, b in -1_000_000_000i128..1_000_000_000, d in 1i128..1_000_000_000) {
            let floor = mul_div(a, b, d).unwrap();
            let nearest = mul_div_nearest(a, b, d).unwrap();
            let ceil = mul_div_up(a, b, d).unwrap();

            prop_assert!(floor <= nearest);
            prop_assert!(nearest <= ceil);

            // Each result is within one unit of the exact rational result.
            let product = a.checked_mul(b).unwrap();
            let exact_num = product;
            // |result * d - product| <= |d|
            let check = |r: i128| {
                let lhs = r.checked_mul(d).unwrap();
                let diff = lhs.checked_sub(exact_num).unwrap();
                diff.checked_abs().unwrap() <= d.checked_abs().unwrap()
            };
            prop_assert!(check(floor));
            prop_assert!(check(nearest));
            prop_assert!(check(ceil));
        }

        /// Scale round-trips: `from_scale(to_scale(x, s), s) == x` when the
        /// scaling multiplication does not overflow.
        #[test]
        fn scale_round_trip(x in -1_000_000i128..1_000_000, s in 1i128..1_000_000) {
            let scaled = to_scale(x, s).unwrap();
            let back = from_scale(scaled, s).unwrap();
            prop_assert_eq!(back, x);
        }

        /// Overflow detection: extreme inputs return `Err` rather than
        /// panicking.
        #[test]
        fn overflow_detected(a in prop_oneof![Just(i128::MAX), Just(i128::MIN), Just(i128::MAX / 2), Just(i128::MIN / 2)], b in prop_oneof![Just(i128::MAX), Just(i128::MIN), Just(2i128), Just(3i128)]) {
            let result = mul_div(a, b, 1);
            // Either it succeeds (no overflow) or returns Err; it must never panic.
            if let Ok(v) = result {
                prop_assert!(v == a.checked_mul(b).unwrap());
            }
        }

        /// Division by zero is always reported as an error.
        #[test]
        fn division_by_zero(a in any::<i128>(), b in any::<i128>()) {
            prop_assert_eq!(mul_div(a, b, 0), Err(FixedPointError::DivisionByZero));
            prop_assert_eq!(mul_div_up(a, b, 0), Err(FixedPointError::DivisionByZero));
            prop_assert_eq!(mul_div_nearest(a, b, 0), Err(FixedPointError::DivisionByZero));
        }

        /// `scale_mul` agrees with `mul_div` using the scale as denominator.
        #[test]
        fn scale_mul_matches_mul_div(a in -1_000_000_000i128..1_000_000_000, b in -1_000_000_000i128..1_000_000_000, s in 1i128..1_000_000_000) {
            prop_assert_eq!(scale_mul(a, b, s), mul_div(a, b, s));
        }

        /// `FixedPoint128` multiplication and division preserve scale.
        #[test]
        fn fixed_point_mul_div_preserve_scale(a in -1_000_000i128..1_000_000, b in 1i128..1_000_000, s in 1i128..1_000_000) {
            let x = FixedPoint128::new(a, s).unwrap();
            let y = FixedPoint128::new(b, s).unwrap();
            let product = x.checked_mul(&y).unwrap();
            prop_assert_eq!(product.scale, s);
            let quotient = x.checked_div(&y).unwrap();
            prop_assert_eq!(quotient.scale, s);
        }
    }

    #[test]
    fn mul_div_basic() {
        assert_eq!(mul_div(6, 7, 2), Ok(21));
        assert_eq!(mul_div(1, 1, 3), Ok(0));
        assert_eq!(mul_div_up(1, 1, 3), Ok(1));
        assert_eq!(mul_div_nearest(1, 1, 3), Ok(0));
        assert_eq!(mul_div_nearest(2, 1, 3), Ok(1));
    }

    #[test]
    fn scale_helpers_basic() {
        assert_eq!(to_scale(3, 1_000_000_000), Ok(3_000_000_000));
        assert_eq!(from_scale(3_000_000_000, 1_000_000_000), Ok(3));
        assert_eq!(scale_mul(2_000_000_000, 3_000_000_000, 1_000_000_000), Ok(6_000_000_000));
        assert_eq!(scale_div(6_000_000_000, 3_000_000_000, 1_000_000_000), Ok(2_000_000_000));
    }

    #[test]
    fn error_cases() {
        assert_eq!(mul_div(1, 1, 0), Err(FixedPointError::DivisionByZero));
        assert_eq!(to_scale(1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(from_scale(1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(scale_mul(1, 1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(scale_div(1, 0, 1), Err(FixedPointError::DivisionByZero));
        assert_eq!(scale_div(1, 1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(mul_div(i128::MAX, i128::MAX, 1), Err(FixedPointError::Overflow));
    }

    #[test]
    fn fixed_sqrt_small_values() {
        assert_eq!(fixed_sqrt(0), Ok(0));
        assert_eq!(fixed_sqrt(1), Ok(1));
        assert_eq!(fixed_sqrt(2), Ok(1));
        assert_eq!(fixed_sqrt(3), Ok(1));
        assert_eq!(fixed_sqrt(4), Ok(2));
        assert_eq!(fixed_sqrt(5), Ok(2));
        assert_eq!(fixed_sqrt(8), Ok(2));
        assert_eq!(fixed_sqrt(9), Ok(3));
        assert_eq!(fixed_sqrt(15), Ok(3));
        assert_eq!(fixed_sqrt(16), Ok(4));
    }

    #[test]
    fn fixed_sqrt_perfect_squares() {
        for r in 0i128..=1_000 {
            let square = r * r;
            assert_eq!(fixed_sqrt(square), Ok(r));
            if square > 0 {
                // One below a perfect square floors to the previous root.
                assert_eq!(fixed_sqrt(square - 1), Ok(r - 1));
            }
            // One above a perfect square still floors to the same root.
            assert_eq!(fixed_sqrt(square + 1), Ok(r));
        }
    }

    #[test]
    fn fixed_sqrt_scaled_edge_cases() {
        // Zero is always zero.
        assert_eq!(fixed_sqrt_scaled(0, 1_000_000_000), Ok(0));
        // One in scale 10^9 is 1e-9; its sqrt is 1e-4.5, so the floor is 0.
        assert_eq!(fixed_sqrt_scaled(1, 1_000_000_000), Ok(0));
        // A perfect square: 4.0 -> 2.0.
        assert_eq!(fixed_sqrt_scaled(4_000_000_000, 1_000_000_000), Ok(2_000_000_000));
        // 9.0 -> 3.0.
        assert_eq!(fixed_sqrt_scaled(9_000_000_000, 1_000_000_000), Ok(3_000_000_000));
        // 1.0 -> 1.0.
        assert_eq!(fixed_sqrt_scaled(1_000_000_000, 1_000_000_000), Ok(1_000_000_000));
        // 0.25 -> 0.5.
        assert_eq!(fixed_sqrt_scaled(250_000_000, 1_000_000_000), Ok(500_000_000));
        // Invalid inputs.
        assert_eq!(fixed_sqrt_scaled(-1, 1_000_000_000), Err(FixedPointError::InvalidScale));
        assert_eq!(fixed_sqrt_scaled(1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(fixed_sqrt_scaled(1, -1), Err(FixedPointError::InvalidScale));
        // Overflow of the radicand.
        assert_eq!(fixed_sqrt_scaled(i128::MAX, 2), Err(FixedPointError::Overflow));
    }

    #[test]
    fn fixed_sqrt_scaled_matches_float() {
        let scales: [i128; 3] = [10_000_000, 1_000_000_000, 1_000_000_000_000_000_000];
        let values: [i128; 8] = [1, 2, 3, 10, 100, 1_000, 123_456, 1_000_000];
        for &scale in scales.iter() {
            for &v in values.iter() {
                let val = v * scale;
                let result = fixed_sqrt_scaled(val, scale).unwrap();
                let expected = (val as f64).sqrt();
                let diff = (result as f64 - expected).abs();
                // Allow a small tolerance for floating-point rounding; the
                // integer result should be within a few units of the float.
                assert!(
                    diff <= 2.0,
                    "scale={} val={} result={} expected={}",
                    scale,
                    val,
                    result,
                    expected
                );
            }
        }
    }

    #[test]
    fn fixed_sqrt_large_values() {
        // Largest root whose square fits in i128.
        let max_root = 13_043_817_825_332_782_212i128;
        assert_eq!(fixed_sqrt(i128::MAX), Ok(max_root));
        assert_eq!(fixed_sqrt(max_root * max_root), Ok(max_root));
        assert_eq!(fixed_sqrt(max_root * max_root - 1), Ok(max_root - 1));
        assert_eq!(fixed_sqrt((max_root + 1) * (max_root + 1) - 1), Ok(max_root));
        assert_eq!(fixed_sqrt(1_000_000_000_000_000_000), Ok(1_000_000_000));
    }

    #[test]
    fn fixed_sqrt_negative_input() {
        assert_eq!(fixed_sqrt(-1), Err(FixedPointError::NegativeInput));
        assert_eq!(fixed_sqrt(i128::MIN), Err(FixedPointError::NegativeInput));
    }

    #[test]
    fn fixed_sqrt_scaled_large_value() {
        // sqrt of a large perfect square in scale 10^9.
        let scale = 1_000_000_000i128;
        let val = 1_000_000_000_000_000_000i128; // 1e9 in scale 1e9
        let result = fixed_sqrt_scaled(val, scale).unwrap();
        let expected = (val as f64).sqrt();
        assert!((result as f64 - expected).abs() <= 2.0);
    }

    #[test]
    fn bancor_pow_basic() {
        let scale = BANCOR_SCALE;
        // Anything to the zeroth power is 1.0.
        assert_eq!(bancor_pow(2 * scale, 0, scale), Ok(scale));
        // 2.0^1 = 2.0.
        assert_eq!(bancor_pow(2 * scale, 1, scale), Ok(2 * scale));
        // 2.0^2 = 4.0.
        assert_eq!(bancor_pow(2 * scale, 2, scale), Ok(4 * scale));
        // 2.0^10 = 1024.0.
        assert_eq!(bancor_pow(2 * scale, 10, scale), Ok(1024 * scale));
        // 1.0^n = 1.0.
        assert_eq!(bancor_pow(scale, 50, scale), Ok(scale));
        // 0.0^n = 0.0 for n > 0.
        assert_eq!(bancor_pow(0, 5, scale), Ok(0));
    }

    #[test]
    fn bancor_pow_error_cases() {
        let scale = BANCOR_SCALE;
        assert_eq!(bancor_pow(scale, 1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(bancor_pow(scale, 1, -1), Err(FixedPointError::InvalidScale));
        assert_eq!(bancor_pow(-1, 1, scale), Err(FixedPointError::NegativeInput));
        assert_eq!(
            bancor_pow(scale, BANCOR_MAX_EXPONENT + 1, scale),
            Err(FixedPointError::Overflow)
        );
        // 2.0^128 overflows i128.
        assert_eq!(bancor_pow(2 * scale, 128, scale), Err(FixedPointError::Overflow));
    }

    #[test]
    fn bancor_price_basic() {
        let scale = BANCOR_SCALE;
        // At zero supply the price equals the base price.
        assert_eq!(bancor_price(scale, 0, 2, scale), Ok(scale));
        // Linear curve: price = base * (1 + supply).
        assert_eq!(bancor_price(scale, scale, 1, scale), Ok(2 * scale));
        assert_eq!(bancor_price(scale, 2 * scale, 1, scale), Ok(3 * scale));
        // Quadratic curve: price = base * (1 + supply)^2.
        assert_eq!(bancor_price(scale, scale, 2, scale), Ok(4 * scale));
    }

    #[test]
    fn bancor_price_error_cases() {
        let scale = BANCOR_SCALE;
        assert_eq!(bancor_price(scale, 0, 1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(bancor_price(-1, 0, 1, scale), Err(FixedPointError::NegativeInput));
        assert_eq!(bancor_price(scale, -1, 1, scale), Err(FixedPointError::NegativeInput));
        assert_eq!(
            bancor_price(scale, 0, BANCOR_MAX_EXPONENT + 1, scale),
            Err(FixedPointError::Overflow)
        );
    }

    #[test]
    fn bancor_integral_basic() {
        let scale = BANCOR_SCALE;
        // Integral of the constant curve price = base over [0, supply] is
        // base * supply.
        assert_eq!(bancor_integral(scale, 0, 0, scale), Ok(0));
        assert_eq!(bancor_integral(scale, scale, 0, scale), Ok(scale));
        // Integral of the linear curve base * (1 + s) over [0, s] is
        // base * (s + s^2 / 2); for s = 1 this is 1.5 * base.
        assert_eq!(
            bancor_integral(scale, scale, 1, scale),
            Ok(scale + scale / 2)
        );
    }

    #[test]
    fn bancor_integral_error_cases() {
        let scale = BANCOR_SCALE;
        assert_eq!(bancor_integral(scale, 0, 1, 0), Err(FixedPointError::InvalidScale));
        assert_eq!(bancor_integral(-1, 0, 1, scale), Err(FixedPointError::NegativeInput));
        assert_eq!(bancor_integral(scale, -1, 1, scale), Err(FixedPointError::NegativeInput));
        assert_eq!(
            bancor_integral(scale, 0, BANCOR_MAX_EXPONENT, scale),
            Err(FixedPointError::Overflow)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// The Bancor price is monotonically non-decreasing in supply.
        #[test]
        fn bancor_price_monotonic(
            base in 1i128..1_000_000_000,
            s1 in 0i128..1_000_000_000,
            delta in 0i128..1_000_000_000,
            exponent in 0u32..8,
        ) {
            let scale = 1_000_000_000i128;
            let s2 = s1 + delta;
            let p1 = bancor_price(base, s1, exponent, scale).unwrap();
            let p2 = bancor_price(base, s2, exponent, scale).unwrap();
            prop_assert!(p1 <= p2, "price decreased: p1={} p2={}", p1, p2);
        }

        /// The Bancor integral is monotonically non-decreasing in supply.
        #[test]
        fn bancor_integral_monotonic(
            base in 1i128..1_000_000_000,
            s1 in 0i128..1_000_000_000,
            delta in 0i128..1_000_000_000,
            exponent in 0u32..6,
        ) {
            let scale = 1_000_000_000i128;
            let s2 = s1 + delta;
            let i1 = bancor_integral(base, s1, exponent, scale).unwrap();
            let i2 = bancor_integral(base, s2, exponent, scale).unwrap();
            prop_assert!(i1 <= i2, "integral decreased: i1={} i2={}", i1, i2);
        }

        /// `bancor_pow` never panics on extreme inputs; it either returns a
        /// value or an error.
        #[test]
        fn bancor_pow_never_panics(
            base in prop_oneof![Just(0i128), Just(1i128), Just(i128::MAX), Just(i128::MAX / 2), Just(BANCOR_SCALE)],
            exponent in 0u32..=BANCOR_MAX_EXPONENT,
        ) {
            let scale = BANCOR_SCALE;
            let _ = bancor_pow(base, exponent, scale);
        }

        /// `bancor_price` and `bancor_integral` never panic on extreme
        /// supplies; they either return a value or an error.
        #[test]
        fn bancor_curve_never_panics(
            supply in prop_oneof![Just(0i128), Just(1i128), Just(i128::MAX), Just(i128::MAX / 2), Just(BANCOR_SCALE)],
            exponent in 0u32..=BANCOR_MAX_EXPONENT,
        ) {
            let scale = BANCOR_SCALE;
            let _ = bancor_price(scale, supply, exponent, scale);
            let _ = bancor_integral(scale, supply, exponent, scale);
        }
    }
}