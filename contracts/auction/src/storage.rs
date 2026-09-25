// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Seller,
    Token,
    StartPrice,
    MinIncrement,
    Deadline,
    HighestBidder,
    HighestBid,
    Settled,
    /// Optional reserve price; auction settles only if highest_bid >= reserve_price.
    ReservePrice,
    /// Pending refund for outbid bidders.
    Pending(Address),
    /// Anti-sniping: number of ledgers to extend the deadline when a bid
    /// arrives within this window of the current deadline.
    ExtensionWindow,
    /// Hard upper bound on `Deadline`: anti-sniping extensions never push the
    /// deadline past this ledger.
    MaxDeadline,
    /// True once the seller has cancelled the auction.
    Cancelled,
    /// Ledger sequence at which `start` was called; anchors the cancellation
    /// grace window.
    StartLedger,
    /// Number of ledgers after `StartLedger` during which the seller may
    /// cancel even though bids exist (0 = disabled).
    CancellationGraceLedgers,
    /// Compensation the seller pays the top bidder when cancelling inside the
    /// grace window after a bid has been placed.
    CancellationFee,
    /// Custodial escrow (issue #1069): NFT contract holding the auctioned item.
    NftContract,
    /// Custodial escrow (issue #1069): token id of the auctioned item.
    NftTokenId,
    /// Dutch (descending price) schedule (issue #1071). Present only when the
    /// auction was started with `start_dutch`.
    DutchConfig,
}

/// Linear price-decay schedule for a Dutch auction (issue #1071).
///
/// The price falls from `start_price` at `start_ledger` to `floor_price` at
/// `start_ledger + duration_ledgers`, and stays at `floor_price` afterwards.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DutchConfig {
    pub start_price: i128,
    pub floor_price: i128,
    pub start_ledger: u32,
    pub duration_ledgers: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct AuctionInfo {
    pub seller: Address,
    pub token: Address,
    pub start_price: i128,
    pub min_increment: i128,
    pub deadline: u32,
    pub highest_bid: i128,
    pub highest_bidder: Option<Address>,
    pub settled: bool,
    /// Optional reserve price set at start.
    pub reserve_price: Option<i128>,
    /// Anti-sniping extension window in ledgers (0 = disabled).
    pub extension_window: u32,
    /// Ledger past which anti-sniping extensions cannot push the deadline.
    pub max_deadline: u32,
    /// Ledger sequence at which the auction was started.
    pub start_ledger: u32,
    /// Seller cancellation grace window in ledgers (0 = disabled).
    pub cancellation_grace_ledgers: u32,
    /// Compensation paid to the top bidder on a grace-window cancellation.
    pub cancellation_fee: i128,
    /// NFT contract of the escrowed item, if the auction is custodial.
    pub nft_contract: Option<Address>,
    /// Token id of the escrowed item, if the auction is custodial.
    pub nft_token_id: Option<u32>,
}
