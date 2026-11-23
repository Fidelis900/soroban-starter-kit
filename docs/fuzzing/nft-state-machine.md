# Fuzz Target: `nft_state_machine`

> Tracks issue **#1366**: libFuzzer invariant target for NFT approval, `transfer_from`, and royalty accounting.

| | |
|---|---|
| **Target file** | `fuzz/fuzz_targets/nft_state_machine.rs` |
| **Contract under test** | `contracts/nft` (`soroban-nft-template`) |
| **Harness style** | Stateful: an `arbitrary`-generated action sequence checked against a shadow ownership model |
| **Run** | `cargo +nightly fuzz run nft_state_machine -- -max_total_time=300` |

---

## 1. Why this target exists

The NFT contract combines several pieces of state that must stay consistent with each other:

- **Ownership.** `Owner(token_id)`, plus per-owner enumerable indexes and `balance_of`.
- **Approvals.** A per-token spender (`approve`) and per-owner operators (`set_approval_for_all`).
- **Royalties.** A per-token override, with a fallback to the collection default.
- **Supply.** `max_supply`, `total_supply`, and `burn`.
- **Soulbound mode.** Every transfer is disabled.

These edge cases come from the issue:

- **Zero prices.** `royalty_info(id, 0)`.
- **Max supply.** Minting at and past `max_supply`.
- **Re-approvals.** Approve A, approve B, then A tries `transfer_from`.

There are also stale approvals after a transfer or burn, and a royalty of exactly `10_000` bps.

---

## 2. Contract API used by the harness

```rust
initialize(admin, name, symbol, max_supply: u32, royalty_bps: Option<u32>,
           royalty_recipient: Option<Address>, soulbound: bool)
mint(to, token_id, token_uri, token_royalty_bps: Option<u32>, token_royalty_recipient: Option<Address>)
mint_batch(..)
transfer(from, to, token_id)
transfer_batch(..)
approve(token_id, spender)                     // owner auth
set_approval_for_all(owner, operator, approved)
transfer_from(spender, from, to, token_id)
burn(..)
royalty_info(token_id, sale_price: i128) -> Option<RoyaltyInfo { recipient, amount }>
owner_of(token_id), get_approved(token_id), is_approved_for_all(owner, operator)
balance_of(owner), token_of_owner_by_index(owner, i), total_supply(), is_soulbound()
```

> **⚠️ Prerequisite: the contract must compile first.** At the time of writing, `contracts/nft/src/lib.rs` contains leftover merge-conflict code. There are two definitions each of `burn`, `approve`, `transfer_from` and `balance_of`, a duplicated `if env.storage().instance().has(..)` line in `initialize`, and duplicated `let amount = ...` lines in `royalty_info`. The two `transfer_from` bodies also disagree:
>
> - one reads `Owner` from **instance** storage, the other from **persistent** storage;
> - one honours `set_approval_for_all`, the other does not.
>
> Settle these conflicts before writing the harness. Then document the intended approval semantics (does an operator approval authorise `transfer_from`?), because invariants N4 and N5 below depend on that answer.

---

## 3. Input model

```rust
#[derive(Arbitrary, Debug)]
struct Input {
    max_supply: u8,                       // 0 = uncapped (if supported), else 1..=32
    collection_royalty: Option<u16>,      // bps, may exceed 10_000 on purpose
    soulbound: bool,
    actions: Vec<Action>,                 // ≤ 128
}

#[derive(Arbitrary, Debug)]
enum Action {
    Mint        { to: u8, id: u8, royalty: Option<u16>, has_recipient: bool },
    Transfer    { from: u8, to: u8, id: u8 },
    Approve     { id: u8, spender: u8 },
    SetOperator { owner: u8, operator: u8, approved: bool },
    TransferFrom { spender: u8, from: u8, to: u8, id: u8 },
    Burn        { who: u8, id: u8 },
    QueryRoyalty { id: u8, price: i128 },  // full i128 range, including 0, negatives, and i128::MAX
}
```

**Mapping rules**

- **Addresses.** Draw from a fixed pool of ≤ 6 addresses. A small pool makes self-transfers, approving yourself, and operator == owner reachable.
- **Token ids.** `u8`, so collisions (`TokenAlreadyExists`) and unknown ids (`TokenNotFound`) are frequent.
- **Royalty bps.** Do not clamp royalty bps. Values `> 10_000` must be rejected with `InvalidRoyalty`.
- **Calls.** Use `try_*` client methods throughout. Update the shadow model only when a call succeeds.

---

## 4. Shadow model

```rust
struct Model {
    owner: BTreeMap<u32, Addr>,
    approved: BTreeMap<u32, Addr>,
    operators: BTreeSet<(Addr, Addr)>,
    token_royalty: BTreeMap<u32, (u32, Option<Addr>)>,
    minted_ever: u32,
}
```

---

## 5. Invariants

Checked after every action.

### Ownership

| # | Invariant |
|---|---|
| **N1** | **Ownership exclusivity.** Every live token has exactly one owner: `owner_of(id)` matches the model, and `id` appears in exactly one owner's enumerable index. |
| **N2** | **Balance consistency.** For every address, `balance_of(a) == |{id : owner(id) == a}|`, and `token_of_owner_by_index(a, i)` for `i < balance_of(a)` lists exactly those ids. |
| **N3** | **Supply.** `total_supply() == Σ balance_of(a) == model.owner.len()`, and `total_supply() ≤ max_supply`. A mint that would exceed `max_supply` fails. |

### Approvals

| # | Invariant |
|---|---|
| **N4** | **Authorised `transfer_from` only.** `transfer_from` succeeds only if the spender is the owner, the current per-token approved spender, or (if the contract honours operators) an operator of the owner. Otherwise it returns `NotApproved` and nothing changes. |
| **N5** | **Re-approval replaces.** After `approve(id, A)` and then `approve(id, B)`, `get_approved(id) == B` and a `transfer_from` by A fails. |
| **N6** | **Approval cleared on move.** After any successful `transfer`, `transfer_from` or `burn` of `id`, `get_approved(id) == None`. A stale spender can never move the token again. |
| **N7** | **Operators survive transfers.** Operator approvals are per owner, not per token. Transferring a token does not grant the old owner's operator any rights over the new owner's tokens. |
| **N8** | **Soulbound.** When `soulbound == true`, every `transfer`, `transfer_from` and `transfer_batch` fails, and ownership never changes after mint. |

### Royalties

| # | Invariant |
|---|---|
| **R1** | **Bounds.** For `sale_price ≥ 0`, `0 ≤ royalty.amount ≤ sale_price`. |
| **R2** | **Zero price.** `royalty_info(id, 0)` returns `None` or an amount of `0`. It never errors or panics. |
| **R3** | **Exact formula.** `amount == floor(sale_price × bps / 10_000)`, where `bps` is the per-token override if one is set and the collection default otherwise. |
| **R4** | **Zero-bps override wins.** A per-token override of `0` bps returns `None` even when a collection default exists. |
| **R5** | **Overflow safety.** With `sale_price` near `i128::MAX`, the call returns a typed error or a saturated or zero result. It never panics. Currently, the collection-default path uses unchecked `sale_price * bps`, which panics on overflow in both debug and release builds, because the workspace sets `overflow-checks = true`. Expect the fuzzer to find this quickly. |
| **R6** | **Negative prices.** A negative `sale_price` is either rejected or produces an amount `≤ 0`. Record the intended behaviour in the contract docs. |
| **R7** | **Recipient present.** Whenever `royalty_info` returns `Some`, its `recipient` is the configured one for that token or collection. |
| **N9** | **No host panic.** Every action ends in `Ok` or a typed `NftError`. |

---

## 6. Registering the target

```toml
[dependencies.soroban-nft-template]
path = "../contracts/nft"

[[bin]]
name = "nft_state_machine"
path = "fuzz_targets/nft_state_machine.rs"
test = false
doc = false
bench = false
```

---

## 7. Running

```bash
cd fuzz
cargo +nightly fuzz run nft_state_machine -- -max_total_time=300 -max_len=8192
```

Seed the corpus with:

- mint up to `max_supply` exactly
- approve A → approve B → `transfer_from` by A
- `royalty_info` with price `0` and with `i128::MAX`

---

## 8. Acceptance checklist

- [ ] NFT contract compiles with a single, documented approval model (see the prerequisite in section 2)
- [ ] `fuzz/fuzz_targets/nft_state_machine.rs` implemented with an `arbitrary` action sequence and a shadow model
- [ ] Ownership exclusivity invariants (N1–N3) and royalty boundary invariants (R1–R7) asserted after every step
- [ ] `soroban-nft-template` dependency and `[[bin]] nft_state_machine` added to `fuzz/Cargo.toml`
- [ ] Five-minute run completes with no crashes, or every crash has been triaged into a regression test
