//! AMM math helpers for PumpSwap (constant-product x·y=k with fee-on-input).
//!
//! Fee model
//! ---------
//! PumpSwap charges fees on the **input** token:
//!   effective_input = input_amount * (10_000 - total_fee_bps) / 10_000
//!
//! where `total_fee_bps = lp_fee_bps + protocol_fee_bps` (fetched from GlobalConfig).
//! Default mainnet values: lp_fee = 25 bps, protocol_fee = 5 bps → total = 30 bps.
//!
//! Swap formula
//! ------------
//!   output = reserve_out * effective_input / (reserve_in + effective_input)
//!
//! All math is done in u128 to prevent overflow, with the result cast back to u64.

use crate::error::{SwapError, SwapResult};

/// Default total fee basis points (lp_fee 25 + protocol_fee 5).
pub const DEFAULT_FEE_BPS: u64 = 30;

// ─────────────────────────────────────────────
// Core AMM formulas
// ─────────────────────────────────────────────

/// Calculate how much **quote** (SOL) you must provide to receive exactly
/// `base_out` base-tokens from a pool.
///
/// Returns `Err(MathOverflow)` on integer overflow or `Err(ZeroLiquidity)` if
/// either reserve is zero, and `Err(InsufficientPoolLiquidity)` if
/// `base_out >= base_reserves`.
pub fn quote_in_for_exact_base_out(
    base_reserves: u64,
    quote_reserves: u64,
    base_out: u64,
    fee_bps: u64,
) -> SwapResult<u64> {
    if base_reserves == 0 || quote_reserves == 0 {
        return Err(SwapError::ZeroLiquidity);
    }
    if base_out >= base_reserves {
        return Err(SwapError::InsufficientPoolLiquidity {
            requested: base_out,
            reserves: base_reserves,
        });
    }

    // Invert the swap formula to find gross quote input:
    //   gross_quote_in = quote_reserves * base_out / (base_reserves - base_out)
    // We add 1 after the division to round up (user pays a tiny bit more).
    let numerator = (quote_reserves as u128)
        .checked_mul(base_out as u128)
        .ok_or(SwapError::MathOverflow)?;
    let denominator = (base_reserves as u128)
        .checked_sub(base_out as u128)
        .ok_or(SwapError::MathOverflow)?;

    // effective_quote_in is the net portion after fee is stripped; we need gross.
    //   effective_in = gross_in * (10000 - fee_bps) / 10000
    //   gross_in = effective_in * 10000 / (10000 - fee_bps)   [round up]
    let effective_in = numerator
        .checked_div(denominator)
        .ok_or(SwapError::MathOverflow)?
        .checked_add(1) // ceil
        .ok_or(SwapError::MathOverflow)?;

    let fee_denom = 10_000u128
        .checked_sub(fee_bps as u128)
        .ok_or(SwapError::MathOverflow)?;

    let gross_in = effective_in
        .checked_mul(10_000)
        .ok_or(SwapError::MathOverflow)?
        .checked_add(fee_denom - 1) // ceil division
        .ok_or(SwapError::MathOverflow)?
        .checked_div(fee_denom)
        .ok_or(SwapError::MathOverflow)?;

    u64::try_from(gross_in).map_err(|_| SwapError::MathOverflow)
}

/// Calculate how much **base** you receive when you sell exactly `base_in`
/// base-tokens into the pool.
pub fn quote_out_for_exact_base_in(
    base_reserves: u64,
    quote_reserves: u64,
    base_in: u64,
    fee_bps: u64,
) -> SwapResult<u64> {
    if base_reserves == 0 || quote_reserves == 0 {
        return Err(SwapError::ZeroLiquidity);
    }

    let effective_in = (base_in as u128)
        .checked_mul(10_000 - fee_bps as u128)
        .ok_or(SwapError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(SwapError::MathOverflow)?;

    let numerator = (quote_reserves as u128)
        .checked_mul(effective_in)
        .ok_or(SwapError::MathOverflow)?;
    let denominator = (base_reserves as u128)
        .checked_add(effective_in)
        .ok_or(SwapError::MathOverflow)?;

    let quote_out = numerator
        .checked_div(denominator)
        .ok_or(SwapError::MathOverflow)?;

    u64::try_from(quote_out).map_err(|_| SwapError::MathOverflow)
}

/// Calculate how much **base** you receive when you provide exactly `quote_in`
/// quote (SOL) to buy.
pub fn base_out_for_exact_quote_in(
    base_reserves: u64,
    quote_reserves: u64,
    quote_in: u64,
    fee_bps: u64,
) -> SwapResult<u64> {
    if base_reserves == 0 || quote_reserves == 0 {
        return Err(SwapError::ZeroLiquidity);
    }

    let effective_in = (quote_in as u128)
        .checked_mul(10_000 - fee_bps as u128)
        .ok_or(SwapError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(SwapError::MathOverflow)?;

    let numerator = (base_reserves as u128)
        .checked_mul(effective_in)
        .ok_or(SwapError::MathOverflow)?;
    let denominator = (quote_reserves as u128)
        .checked_add(effective_in)
        .ok_or(SwapError::MathOverflow)?;

    let base_out = numerator
        .checked_div(denominator)
        .ok_or(SwapError::MathOverflow)?;

    u64::try_from(base_out).map_err(|_| SwapError::MathOverflow)
}

// ─────────────────────────────────────────────
// Slippage helpers
// ─────────────────────────────────────────────

/// Apply a slippage tolerance **upward** — for amounts you're willing to pay at
/// most (e.g., `max_quote_amount_in` on a buy).
///
/// `slippage_bps = 50` → 0.5% tolerance.
pub fn with_slippage_max(amount: u64, slippage_bps: u64) -> SwapResult<u64> {
    let result = (amount as u128)
        .checked_mul(10_000 + slippage_bps as u128)
        .ok_or(SwapError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(SwapError::MathOverflow)?;
    u64::try_from(result).map_err(|_| SwapError::MathOverflow)
}

/// Apply a slippage tolerance **downward** — for amounts you require at minimum
/// (e.g., `min_quote_amount_out` on a sell).
pub fn with_slippage_min(amount: u64, slippage_bps: u64) -> SwapResult<u64> {
    let result = (amount as u128)
        .checked_mul(10_000 - slippage_bps as u128)
        .ok_or(SwapError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(SwapError::MathOverflow)?;
    u64::try_from(result).map_err(|_| SwapError::MathOverflow)
}

// ─────────────────────────────────────────────
// Price helpers
// ─────────────────────────────────────────────

/// Spot price of base in terms of quote (lamports per base atom), scaled by
/// `decimals_scale = 10^(quote_decimals - base_decimals)`.
///
/// Returns `None` if `base_reserves` is zero.
pub fn spot_price_quote_per_base(base_reserves: u64, quote_reserves: u64) -> Option<f64> {
    if base_reserves == 0 {
        return None;
    }
    Some(quote_reserves as f64 / base_reserves as f64)
}

/// Price impact in basis points for a buy of `quote_in`.
pub fn price_impact_bps_buy(
    base_reserves: u64,
    quote_reserves: u64,
    quote_in: u64,
) -> SwapResult<u64> {
    if base_reserves == 0 || quote_reserves == 0 {
        return Err(SwapError::ZeroLiquidity);
    }
    let spot = quote_reserves as f64 / base_reserves as f64;
    let new_quote = quote_reserves as f64 + quote_in as f64;
    let new_base = (base_reserves as f64 * quote_reserves as f64) / new_quote;
    let exec_price = quote_in as f64 / (base_reserves as f64 - new_base);
    let impact = ((exec_price - spot) / spot * 10_000.0).max(0.0);
    Ok(impact as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEE: u64 = 30; // 0.30%

    #[test]
    fn round_trip_buy_sell() {
        let base_r = 1_000_000_000_u64; // 1B base atoms
        let quote_r = 10_000_000_000_u64; // 10B lamports

        let base_out = 1_000_000_u64;
        let quote_needed =
            quote_in_for_exact_base_out(base_r, quote_r, base_out, FEE).unwrap();

        // quote_needed should be slightly more than spot
        let spot = quote_r as f64 / base_r as f64;
        let min_expected = (spot * base_out as f64) as u64;
        assert!(
            quote_needed >= min_expected,
            "quote_needed {quote_needed} < spot estimate {min_expected}"
        );
    }

    #[test]
    fn sell_reduces_quote_reserves() {
        let base_r = 1_000_000_000_u64;
        let quote_r = 10_000_000_000_u64;
        let base_in = 500_000_u64;
        let q_out = quote_out_for_exact_base_in(base_r, quote_r, base_in, FEE).unwrap();
        assert!(q_out < quote_r);
        assert!(q_out > 0);
    }

    #[test]
    fn slippage_max_increases_amount() {
        let amount = 1_000_000_u64;
        let max = with_slippage_max(amount, 100).unwrap(); // 1%
        assert_eq!(max, 1_010_000);
    }

    #[test]
    fn slippage_min_decreases_amount() {
        let amount = 1_000_000_u64;
        let min = with_slippage_min(amount, 100).unwrap(); // 1%
        assert_eq!(min, 990_000);
    }
}
