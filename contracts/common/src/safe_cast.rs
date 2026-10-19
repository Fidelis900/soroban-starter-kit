use soroban_sdk::contracterror;

/// Errors that can occur when performing checked integer casts.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CastError {
    /// The value is negative and cannot be represented in an unsigned type.
    Negative = 1,
    /// The value exceeds the maximum value representable by the target type.
    Overflow = 2,
    /// The value is outside the representable range of the target type.
    OutOfRange = 3,
}

/// Infallibly widens a `u32` to an `i128`.
///
/// Every `u32` value fits within an `i128`, so this conversion can never fail.
pub fn to_i128(val: u32) -> i128 {
    val as i128
}

/// Converts an `i128` to a `u32`, returning an error if the value is negative
/// or exceeds `u32::MAX`.
pub fn to_u32(val: i128) -> Result<u32, CastError> {
    if val < 0 {
        return Err(CastError::Negative);
    }
    if val > u32::MAX as i128 {
        return Err(CastError::Overflow);
    }
    Ok(val as u32)
}

/// Converts an `i128` to a `u64`, returning an error if the value is negative
/// or exceeds `u64::MAX`.
pub fn to_u64(val: i128) -> Result<u64, CastError> {
    if val < 0 {
        return Err(CastError::Negative);
    }
    if val > u64::MAX as i128 {
        return Err(CastError::Overflow);
    }
    Ok(val as u64)
}

/// Converts an `i128` to a `usize`, returning an error if the value is negative
/// or exceeds `usize::MAX`.
pub fn to_usize(val: i128) -> Result<usize, CastError> {
    if val < 0 {
        return Err(CastError::Negative);
    }
    if val > usize::MAX as i128 {
        return Err(CastError::Overflow);
    }
    Ok(val as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_i128_widens_u32() {
        assert_eq!(to_i128(0), 0i128);
        assert_eq!(to_i128(1), 1i128);
        assert_eq!(to_i128(u32::MAX), u32::MAX as i128);
    }

    #[test]
    fn to_u32_accepts_in_range() {
        assert_eq!(to_u32(0), Ok(0u32));
        assert_eq!(to_u32(1), Ok(1u32));
        assert_eq!(to_u32(u32::MAX as i128), Ok(u32::MAX));
    }

    #[test]
    fn to_u32_rejects_negative() {
        assert_eq!(to_u32(-1), Err(CastError::Negative));
        assert_eq!(to_u32(i128::MIN), Err(CastError::Negative));
    }

    #[test]
    fn to_u32_rejects_overflow() {
        assert_eq!(to_u32(u32::MAX as i128 + 1), Err(CastError::Overflow));
        assert_eq!(to_u32(i128::MAX), Err(CastError::Overflow));
    }

    #[test]
    fn to_u64_accepts_in_range() {
        assert_eq!(to_u64(0), Ok(0u64));
        assert_eq!(to_u64(1), Ok(1u64));
        assert_eq!(to_u64(u64::MAX as i128), Ok(u64::MAX));
    }

    #[test]
    fn to_u64_rejects_negative() {
        assert_eq!(to_u64(-1), Err(CastError::Negative));
        assert_eq!(to_u64(i128::MIN), Err(CastError::Negative));
    }

    #[test]
    fn to_u64_rejects_overflow() {
        assert_eq!(to_u64(u64::MAX as i128 + 1), Err(CastError::Overflow));
        assert_eq!(to_u64(i128::MAX), Err(CastError::Overflow));
    }

    #[test]
    fn to_usize_accepts_in_range() {
        assert_eq!(to_usize(0), Ok(0usize));
        assert_eq!(to_usize(1), Ok(1usize));
        assert_eq!(to_usize(usize::MAX as i128), Ok(usize::MAX));
    }

    #[test]
    fn to_usize_rejects_negative() {
        assert_eq!(to_usize(-1), Err(CastError::Negative));
        assert_eq!(to_usize(i128::MIN), Err(CastError::Negative));
    }

    #[test]
    fn to_usize_rejects_overflow() {
        assert_eq!(to_usize(usize::MAX as i128 + 1), Err(CastError::Overflow));
        assert_eq!(to_usize(i128::MAX), Err(CastError::Overflow));
    }
}