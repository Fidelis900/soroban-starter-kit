#![no_std]
#![deny(missing_docs)]
//! Shared helpers for the Soroban contract templates.
//!
//! Provides admin-storage helpers, TTL/lifetime constants, deadline validation,
//! basis-point fee calculation, commit-reveal hashing, and the [`AdminKey`]
//! storage key reused across the contract crates.

use soroban_sdk::{Address, Bytes, BytesN, Env, contracttype, crypto::Hash};

/// Minimum number of ledgers the deadline must be ahead of the current ledger
/// when initializing an escrow. Enforced by the contract; tests must respect
/// this value to avoid generating deadlines the contract would reject.
pub const MIN_DEADLINE_BUFFER: u32 = 10;

/// Storage key for the contract administrator address.
///
/// Used in instance storage to persist the admin [`Address`] across invocations.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{Env, Address};
/// use soroban_common::AdminKey;
///
/// let env = Env::default();
/// let admin_address = Address::generate(&env);
/// env.storage().instance().set(&AdminKey::Admin, &admin_address);
/// ```
pub use admin_key::AdminKey;

// `#[contracttype]` generates an undocumented public associated item; confine
// the missing_docs allowance to this module and re-export `AdminKey` above.
mod admin_key {
    #![allow(missing_docs)]
    use super::contracttype;

    #[contracttype]
    #[derive(Clone)]
    pub enum AdminKey {
        /// Instance-storage slot holding the administrator [`Address`].
        Admin,
    }
}

/// Reads `AdminKey::Admin` from instance storage, panicking if unset.
///
/// # Panics
///
/// Panics with `"admin not set"` if the admin has not been stored yet.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{Env, Address};
/// use soroban_common::{AdminKey, get_admin};
///
/// let env = Env::default();
/// let admin_address = Address::generate(&env);
/// env.storage().instance().set(&AdminKey::Admin, &admin_address);
///
/// let admin: Address = get_admin(&env);
/// assert_eq!(admin, admin_address);
/// ```
#[must_use]
pub fn get_admin(env: &Env) -> Address {
    #[allow(clippy::expect_used)] // intentional panic: contract invariant
    env.storage()
        .instance()
        .get(&AdminKey::Admin)
        .expect("admin not set")
}

/// Reads `AdminKey::Admin` from instance storage, returning `None` if unset.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{Env, Address};
/// use soroban_common::{AdminKey, try_get_admin};
///
/// let env = Env::default();
///
/// // Before setting admin
/// assert_eq!(try_get_admin(&env), None);
///
/// // After setting admin
/// let admin_address = Address::generate(&env);
/// env.storage().instance().set(&AdminKey::Admin, &admin_address);
/// assert_eq!(try_get_admin(&env), Some(admin_address));
/// ```
#[must_use]
pub fn try_get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&AdminKey::Admin)
}

/// Reads a value from instance storage by key, panicking if missing.
///
/// # Panics
///
/// Panics with `"key not found"` if the key does not exist in instance storage.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{contracttype, Env};
/// use soroban_common::get_instance;
///
/// #[contracttype]
/// #[derive(Clone)]
/// enum DataKey {
///     Amount,
/// }
///
/// let env = Env::default();
/// let amount: i128 = 1000;
/// env.storage().instance().set(&DataKey::Amount, &amount);
///
/// let retrieved: i128 = get_instance(&env, &DataKey::Amount);
/// assert_eq!(retrieved, 1000);
/// ```
pub fn get_instance<K, V>(env: &Env, key: &K) -> V
where
    K: soroban_sdk::TryIntoVal<Env, soroban_sdk::Val> + soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
{
    #[allow(clippy::expect_used)] // intentional panic: contract invariant
    env.storage().instance().get(key).expect("key not found")
}

/// Extends the TTL of instance storage by `extend_to` ledgers if the current
/// TTL is below `threshold`.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::Env;
/// use soroban_common::extend_ttl_instance;
///
/// let env = Env::default();
/// // Keep instance storage alive for ~30 days if TTL drops below ~7 days.
/// extend_ttl_instance(&env, 120_960, 518_400);
/// ```
pub fn extend_ttl_instance(env: &Env, threshold: u32, extend_to: u32) {
    env.storage().instance().extend_ttl(threshold, extend_to);
}

/// Extends the TTL of a persistent storage entry if the current TTL is below
/// `threshold`.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{contracttype, Env, Address};
/// use soroban_common::extend_ttl_persistent;
///
/// #[contracttype]
/// #[derive(Clone)]
/// enum DataKey {
///     Balance(Address),
/// }
///
/// let env = Env::default();
/// let addr = Address::generate(&env);
/// extend_ttl_persistent(&env, &DataKey::Balance(addr), 120_960, 518_400);
/// ```
pub fn extend_ttl_persistent<K>(env: &Env, key: &K, threshold: u32, extend_to: u32)
where
    K: soroban_sdk::TryIntoVal<Env, soroban_sdk::Val> + soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, threshold, extend_to);
}

/// Expected wall-clock seconds between consecutive Soroban ledgers.
/// Used to convert ledger counts to approximate durations in doc comments.
pub const LEDGER_SECONDS: u32 = 5;

/// Ledger threshold for TTL extension (~7 days at `LEDGER_SECONDS` seconds per ledger).
/// When remaining TTL falls below this, storage is extended to `LEDGER_BUMP_AMOUNT`.
pub const LEDGER_LIFETIME_THRESHOLD: u32 = 120_960;

/// Target TTL (in ledgers) after each extension (~30 days at `LEDGER_SECONDS` seconds per ledger).
pub const LEDGER_BUMP_AMOUNT: u32 = 518_400;

/// Generates a `core::fmt::Display` implementation for a `#[contracterror]` enum.
///
/// Soroban contracts are compiled with `#![no_std]`, so `thiserror` (which
/// requires `std`) cannot be used. This macro eliminates the repetitive
/// `match` arms that would otherwise appear in every error module.
///
/// # Usage
///
/// ```ignore
/// use soroban_common::impl_display_error;
///
/// #[contracterror]
/// #[derive(Clone, Copy, Debug)]
/// pub enum MyError {
///     Foo = 1,
///     Bar = 2,
/// }
///
/// impl_display_error!(MyError, Foo => "foo happened", Bar => "bar happened");
/// ```
#[macro_export]
macro_rules! impl_display_error {
    ($err:ty, $($variant:ident => $msg:literal),+ $(,)?) => {
        impl core::fmt::Display for $err {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self {
                    $( <$err>::$variant => f.write_str($msg), )+
                }
            }
        }
    };
}

/// Validates that a deadline is sufficiently far in the future.
///
/// Returns `Ok(())` if `deadline >= current_ledger + MIN_DEADLINE_BUFFER`.
/// Returns an error otherwise.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::validate_deadline;
/// use soroban_sdk::Env;
///
/// let env = Env::default();
/// let deadline = env.ledger().sequence() + 10;
/// validate_deadline(&env, deadline)?; // Ok if MIN_DEADLINE_BUFFER <= 10
/// ```
pub fn validate_deadline<E>(env: &Env, deadline: u32) -> Result<(), E>
where
    E: From<()>,
{
    if deadline < env.ledger().sequence() + MIN_DEADLINE_BUFFER {
        Err(E::from(()))
    } else {
        Ok(())
    }
}

// ─── Basis-point fee calculation (#884) ───────────────────────────────────────

/// Basis-point denominator (10 000 = 100 %).
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Compute a fee as `amount * bps / 10_000` with overflow-safe checked
/// arithmetic.
///
/// Returns `Some(fee)` when the multiplication does not overflow, or `None` if
/// `amount * bps` would exceed `i128::MAX`.  Division truncates toward zero,
/// matching the existing convention across the codebase.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::apply_bps_fee;
///
/// // 5 % fee on 1_000_000
/// assert_eq!(apply_bps_fee(1_000_000, 500), Some(50_000));
/// // 0 bps → zero fee
/// assert_eq!(apply_bps_fee(1_000_000, 0), Some(0));
/// // 10 000 bps → full amount
/// assert_eq!(apply_bps_fee(1_000_000, 10_000), Some(1_000_000));
/// // Overflow → None
/// assert_eq!(apply_bps_fee(i128::MAX, 10_000), None);
/// ```
#[must_use]
pub fn apply_bps_fee(amount: i128, bps: u32) -> Option<i128> {
    amount.checked_mul(bps as i128)?.checked_div(BPS_DENOMINATOR)
}

// ─── Commit-reveal hashing helpers (#887) ────────────────────────────────────

/// Build the SHA-256 preimage for a commit-reveal scheme.
///
/// Concatenates `secret` and `salt` into a single `Bytes` buffer suitable for
/// hashing.  Both inputs are 32-byte values (`BytesN<32>`).
///
/// The returned `Hash<32>` is `SHA-256(secret || salt)`.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::commit_hash;
/// use soroban_sdk::{Env, BytesN};
///
/// let env = Env::default();
/// let secret = BytesN::from_array(&env, &[1u8; 32]);
/// let salt = BytesN::from_array(&env, &[2u8; 32]);
/// let hash = commit_hash(&env, &secret, &salt);
/// ```
#[must_use]
pub fn commit_hash(env: &Env, secret: &BytesN<32>, salt: &BytesN<32>) -> Hash<32> {
    let mut preimage = Bytes::new(env);
    preimage.extend_from_array(&secret.to_array());
    preimage.extend_from_array(&salt.to_array());
    env.crypto().sha256(&preimage)
}

/// Build the entropy input for winner selection in a commit-reveal lottery.
///
/// Concatenates `secret`, `salt`, and an arbitrary-length `extra` byte slice
/// into a single buffer and returns its SHA-256 hash.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::entropy_hash;
/// use soroban_sdk::{Env, BytesN};
///
/// let env = Env::default();
/// let secret = BytesN::from_array(&env, &[1u8; 32]);
/// let salt = BytesN::from_array(&env, &[2u8; 32]);
/// let ledger_bytes = 42u32.to_be_bytes();
/// let hash = entropy_hash(&env, &secret, &salt, &ledger_bytes);
/// ```
#[must_use]
pub fn entropy_hash(env: &Env, secret: &BytesN<32>, salt: &BytesN<32>, extra: &[u8]) -> Hash<32> {
    let mut input = Bytes::new(env);
    input.extend_from_array(&secret.to_array());
    input.extend_from_array(&salt.to_array());
    input.append(&Bytes::from_slice(env, extra));
    env.crypto().sha256(&input)
}

/// Derive a deterministic pool index from a 32-byte SHA-256 hash.
///
/// Takes the **last 8 bytes** of `hash` as a big-endian `u64` and reduces it
/// modulo `pool_len`, returning the result as a `u32` index.
///
/// `pool_len` must be greater than zero; the caller is responsible for
/// validating that constraint.
///
/// # Panics
///
/// Panics if `pool_len == 0`.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::entropy_index;
/// use soroban_sdk::{Env, BytesN};
///
/// let env = Env::default();
/// let secret = BytesN::from_array(&env, &[1u8; 32]);
/// let salt = BytesN::from_array(&env, &[2u8; 32]);
/// let hash = soroban_common::commit_hash(&env, &secret, &salt);
/// let idx = entropy_index(&hash, 10); // 0 <= idx < 10
/// ```
#[allow(clippy::indexing_slicing)] // sha256 output is always 32 bytes
#[must_use]
pub fn entropy_index(hash: &Hash<32>, pool_len: u32) -> u32 {
    let bytes = hash.to_array();
    let idx_raw = u64::from_be_bytes([
        bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
    ]);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        clippy::arithmetic_side_effects,
        clippy::integer_division
    )]
    let idx = (idx_raw % pool_len as u64) as u32;
    idx
}

// ─── Per-address rate limiting (#824) ────────────────────────────────────────

/// Check whether `address` has exceeded `max_calls` within a rolling ledger
/// `window` and, if not, record this call.
///
/// ## Storage
///
/// One **persistent** key per `(namespace, address)` pair is written.  The key
/// is a two-element tuple `(namespace, address)` so that callers can isolate
/// multiple independent rate-limit rules on the same address by choosing
/// different `namespace` values.
///
/// ## Window semantics
///
/// The window is **sliding from the first call**: on the first call for an
/// address the current ledger is recorded as `window_start`.  Subsequent calls
/// within `[window_start, window_start + window)` increment the counter.  Once
/// the ledger advances past `window_start + window` the counter resets and a
/// new window begins.
///
/// ## Parameters
///
/// - `env`       — the contract [`Env`].
/// - `namespace` — a `u32` discriminant acting as a namespace for the stored
///                 key.  Use different values to enforce separate rate-limit
///                 rules for the same address (e.g. `1` for deposits, `2` for
///                 withdrawals).
/// - `address`   — the address to throttle.
/// - `window`    — size of the sliding window in ledgers.  Must be ≥ 1.
/// - `max_calls` — maximum number of allowed calls within a single window.
///
/// ## Returns
///
/// `true`  — the call is **allowed** (counter has been incremented).  
/// `false` — the call is **denied** (limit already reached in this window).
///
/// ## Example
///
/// ```ignore
/// use soroban_common::check_and_record;
/// use soroban_sdk::{Address, Env};
///
/// // Allow at most 3 calls per 100 ledgers for a given address.
/// pub fn sensitive_action(env: Env, caller: Address) -> Result<(), MyError> {
///     if !check_and_record(&env, 1u32, &caller, 100, 3) {
///         return Err(MyError::RateLimitExceeded);
///     }
///     // ... action logic ...
///     Ok(())
/// }
/// ```
pub fn check_and_record(
    env: &Env,
    namespace: u32,
    address: &Address,
    window: u32,
    max_calls: u32,
) -> bool {
    // Composite storage key: (namespace, address).
    let key = (namespace, address.clone());

    let current_ledger = env.ledger().sequence();

    // Load existing entry (window_start, call_count) from persistent storage.
    let entry: Option<(u32, u32)> = env.storage().persistent().get(&key);

    let (window_start, call_count) = match entry {
        Some((ws, cc)) => {
            let window_end = ws.saturating_add(window);
            if current_ledger >= window_end {
                // Window has expired — start a fresh window.
                (current_ledger, 0u32)
            } else {
                (ws, cc)
            }
        }
        None => (current_ledger, 0u32),
    };

    if call_count >= max_calls {
        return false;
    }

    let new_count = call_count.saturating_add(1);
    env.storage()
        .persistent()
        .set(&key, &(window_start, new_count));
    env.storage()
        .persistent()
        .extend_ttl(&key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);

    true
}

// ─── Cursor-based pagination ──────────────────────────────────────────────────

/// A single page of results produced by [`paginate`].
///
/// `T` is the item type.  `C` is the cursor type (must be `Clone + PartialOrd`).
///
/// # Usage
///
/// ```ignore
/// use soroban_common::Page;
/// use soroban_sdk::{Env, Vec};
///
/// // A page containing two items, with a cursor to fetch the next page.
/// let page: Page<u64, u64> = Page {
///     items: soroban_sdk::Vec::new(&env),
///     next_cursor: Some(10u64),
/// };
/// ```
// Note: `#[contracttype]` does not support generic structs, so `Page` is a
// plain Rust type. Contracts that want to expose a page over the contract
// spec should define a concrete `#[contracttype]` wrapper for their `T`/`C`.
#[derive(Clone)]
pub struct Page<T, C>
where
    T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>
        + Clone,
    C: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>
        + Clone,
{
    /// The items in this page, in the order they were yielded by the iterator.
    pub items: soroban_sdk::Vec<T>,
    /// The cursor value to pass to the next call to continue scanning, or
    /// `None` if the end of the range has been reached.
    pub next_cursor: Option<C>,
}

/// Advance a cursor through an ordered sequence and collect at most `limit` items.
///
/// This is the shared implementation that every contract's `get_*` endpoint can
/// delegate to instead of reimplementing the same loop.
///
/// # Parameters
///
/// - `env`        — the contract [`Env`].
/// - `cursor`     — the first cursor value to visit.  Pass the value returned
///                  in `Page::next_cursor` from the previous call to continue
///                  paginating; pass the sentinel start value (e.g. `0u64`) to
///                  start from the beginning.
/// - `limit`      — maximum number of items to include in the returned page.
///                  Clamped to `[1, max_page_size]`.
/// - `max_page_size` — hard upper bound on items per page enforced by the
///                  caller's contract.
/// - `next_cursor_fn` — a closure `(C) -> Option<C>` that advances the cursor
///                  by one step. Returning `None` signals the end of the range.
/// - `fetch_fn`   — a closure `(C) -> Option<T>` that loads an item for the
///                  given cursor, or `None` if no item exists at that position
///                  (i.e. the slot is empty or the item should be skipped).
///
/// # Returns
///
/// A [`Page`] whose `items` contains at most `limit` accepted items and whose
/// `next_cursor` contains the cursor that was *not yet visited* when the page
/// filled up, or `None` once the iterator is exhausted.
///
/// # Examples
///
/// ```ignore
/// use soroban_common::{Page, paginate};
/// use soroban_sdk::Env;
///
/// let env = Env::default();
/// // Enumerate u64 IDs 0..next_id, return active ones.
/// let next_id: u64 = 20;
/// let page: Page<u64, u64> = paginate(
///     &env,
///     /* cursor  */ 0u64,
///     /* limit   */ 10,
///     /* max     */ 50,
///     /* advance */ |c| if c + 1 < next_id { Some(c + 1) } else { None },
///     /* fetch   */ |c| if c % 2 == 0 { Some(c) } else { None },
/// );
/// ```
pub fn paginate<T, C, AdvanceFn, FetchFn>(
    env: &Env,
    cursor: C,
    limit: u32,
    max_page_size: u32,
    advance_fn: AdvanceFn,
    fetch_fn: FetchFn,
) -> Page<T, C>
where
    T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>
        + Clone,
    C: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>
        + Clone,
    AdvanceFn: Fn(C) -> Option<C>,
    FetchFn: Fn(C) -> Option<T>,
{
    let capped = limit.clamp(1, max_page_size);
    let mut items = soroban_sdk::Vec::new(env);
    let mut current: Option<C> = Some(cursor);
    let mut next_cursor: Option<C> = None;

    loop {
        let c = match current {
            Some(ref v) => v.clone(),
            None => break,
        };

        // Page is full — record where to resume.
        if items.len() >= capped {
            next_cursor = Some(c);
            break;
        }

        if let Some(item) = fetch_fn(c.clone()) {
            items.push_back(item);
        }

        current = advance_fn(c);
    }

    Page { items, next_cursor }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── apply_bps_fee tests ──────────────────────────────────────────────

    #[test]
    fn bps_fee_zero_bps() {
        assert_eq!(apply_bps_fee(1_000_000, 0), Some(0));
    }

    #[test]
    fn bps_fee_full_bps() {
        assert_eq!(apply_bps_fee(1_000_000, 10_000), Some(1_000_000));
    }

    #[test]
    fn bps_fee_half() {
        assert_eq!(apply_bps_fee(1_000_000, 5_000), Some(500_000));
    }

    #[test]
    fn bps_fee_typical() {
        // 5 % fee
        assert_eq!(apply_bps_fee(1_000_000, 500), Some(50_000));
    }

    #[test]
    fn bps_fee_truncation() {
        // 1 bp on 1 unit → 0 (integer truncation)
        assert_eq!(apply_bps_fee(1, 1), Some(0));
    }

    #[test]
    fn bps_fee_overflow() {
        assert_eq!(apply_bps_fee(i128::MAX, 10_000), None);
    }

    #[test]
    fn bps_fee_large_amount() {
        // 1_000_000_000_000 * 250 / 10_000 = 25_000_000_000
        assert_eq!(apply_bps_fee(1_000_000_000_000, 250), Some(25_000_000_000));
    }

    #[test]
    fn bps_fee_zero_amount() {
        assert_eq!(apply_bps_fee(0, 500), Some(0));
    }
}

// ─── Shared instance bump helper ────────────────────────────────────────────

/// Extend instance storage TTL using the default threshold and bump amount.
///
/// This is the standard bump that most contracts call after every state-changing
/// entry point.  Delegates to [`extend_ttl_instance`] with
/// [`LEDGER_LIFETIME_THRESHOLD`] and [`LEDGER_BUMP_AMOUNT`].
///
/// # Examples
///
/// ```ignore
/// use soroban_common::bump_instance;
/// use soroban_sdk::Env;
///
/// let env = Env::default();
/// bump_instance(&env);
/// ```
pub fn bump_instance(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

// ─── Generic get_required helper ────────────────────────────────────────────

/// Reads a value from instance storage, returning an error if the key is missing.
///
/// This is the generic version that callers can use with any error type that
/// implements `From<()>`.  For contracts that define a `NotInitialized` variant,
/// the pattern is:
///
/// ```ignore
/// fn get_required<T>(env: &Env, key: &DataKey) -> Result<T, MyError> {
///     soroban_common::get_required(env, key)
/// }
/// ```
///
/// # Panics
///
/// Panics if the key is not present in instance storage.
///
/// # Examples
///
/// ```ignore
/// use soroban_sdk::{contracttype, Env};
/// use soroban_common::get_required;
///
/// #[contracttype]
/// #[derive(Clone)]
/// enum DataKey {
///     Admin,
/// }
///
/// let env = Env::default();
/// // Assuming admin has been set:
/// // let admin: Address = get_required(&env, &DataKey::Admin)?;
/// ```
pub fn get_required<K, V, E>(env: &Env, key: &K) -> Result<V, E>
where
    K: soroban_sdk::TryIntoVal<Env, soroban_sdk::Val> + soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    E: From<()>,
{
    env.storage()
        .instance()
        .get(key)
        .ok_or(E::from(()))
}

// ─── Token storage key ──────────────────────────────────────────────────────

/// Reusable storage key variants shared across contracts.
///
/// Many contracts store an `Admin` and/or a `Token` address in instance
/// storage.  Rather than each crate re-declaring the same enum, they can
/// import the variants from here.
///
/// # Usage
///
/// ```ignore
/// use soroban_common::{CommonKey, get_admin};
///
/// let admin = get_admin(&env);
/// env.storage().instance().set(&CommonKey::Token, &token_address);
/// ```
pub use common_key::CommonKey;

mod common_key {
    #![allow(missing_docs)]
    use super::contracttype;

    /// Shared storage key variants.
    #[contracttype]
    #[derive(Clone)]
    pub enum CommonKey {
        /// Instance-storage slot holding the administrator [`Address`].
        Admin,
        /// Instance-storage slot holding the payment token [`Address`].
        Token,
    }
}
