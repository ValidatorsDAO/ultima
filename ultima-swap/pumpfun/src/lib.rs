//! # ultima-swap-pumpfun
//!
//! Rust SDK for the [PumpSwap AMM](https://pump.fun) on Solana.
//!
//! PumpSwap is Pump.fun's native constant-product AMM for tokens that have
//! graduated from the bonding curve.  This crate provides:
//!
//! - **[`constants`]** — Program IDs, discriminators, PDA seeds, fee recipients.
//! - **[`accounts`]** — `Pool` and `GlobalConfig` structs with Borsh
//!   deserialization and PDA derivation helpers.
//! - **[`instructions`]** — Instruction builders for `buy`, `sell`, and
//!   `create_pool` detection.
//! - **[`math`]** — Constant-product AMM math: exact-in/out quotes, slippage,
//!   spot price, price impact.
//! - **[`error`]** — Typed error enum and `SwapResult<T>` alias.
//!
//! ## Quick example — buy
//!
//! ```rust,no_run
//! use ultima_swap_pumpfun::{
//!     accounts::Pool,
//!     instructions::{build_buy, BuyParams},
//!     math::{quote_in_for_exact_base_out, with_slippage_max, DEFAULT_FEE_BPS},
//! };
//! use solana_pubkey::Pubkey;
//!
//! // Fetch pool account data from RPC (not shown).
//! let pool_address: Pubkey = todo!();
//! let pool_data: Pool = todo!();
//! let user: Pubkey = todo!();
//!
//! // How much SOL is needed to buy 1_000_000 base atoms?
//! let base_reserves: u64 = todo!(); // from token vault balance
//! let quote_reserves: u64 = todo!(); // from SOL vault balance
//!
//! let quote_needed = quote_in_for_exact_base_out(
//!     base_reserves,
//!     quote_reserves,
//!     1_000_000,
//!     DEFAULT_FEE_BPS,
//! ).expect("math overflow");
//!
//! // Add 1% slippage tolerance.
//! let max_quote = with_slippage_max(quote_needed, 100).expect("overflow");
//!
//! let ix = build_buy(BuyParams {
//!     pool: pool_address,
//!     pool_data,
//!     user,
//!     base_amount_out: 1_000_000,
//!     max_quote_amount_in: max_quote,
//!     fee_recipient_index: 0, // rotate across 0–7 for throughput
//! }).expect("build buy");
//! ```

pub mod accounts;
pub mod constants;
pub mod error;
pub mod instructions;
pub mod math;

// Flat re-exports for ergonomic use.
pub use accounts::{derive_event_authority, derive_global_config, GlobalConfig, Pool};
pub use solana_instruction::Instruction;
pub use constants::*;
pub use error::{SwapError, SwapResult};
pub use instructions::{
    build_buy, build_sell, create_ata_if_needed, create_base_ata_if_needed,
    create_quote_ata_if_needed, try_parse_create_pool, try_parse_swap,
    BuyParams, CreatePoolDetected, SellParams, SwapDetected, SwapDirection,
};
pub use math::{
    base_out_for_exact_quote_in, price_impact_bps_buy, quote_in_for_exact_base_out,
    quote_out_for_exact_base_in, spot_price_quote_per_base, with_slippage_max,
    with_slippage_min, DEFAULT_FEE_BPS,
};
