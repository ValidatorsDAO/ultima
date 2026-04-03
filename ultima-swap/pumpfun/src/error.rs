use thiserror::Error;

/// All errors that can be returned by the ultima-swap-pumpfun SDK.
#[derive(Debug, Error)]
pub enum SwapError {
    /// Slippage guard triggered on a buy (max_quote_amount_in exceeded).
    #[error("Buy slippage exceeded: need {needed} lamports but max allowed is {max}")]
    BuySlippageExceeded { needed: u64, max: u64 },

    /// Slippage guard triggered on a sell (min_quote_amount_out not met).
    #[error("Sell slippage exceeded: would receive {received} lamports but min required is {min}")]
    SellSlippageExceeded { received: u64, min: u64 },

    /// The 8-byte Anchor discriminator did not match the expected value.
    #[error("Invalid account discriminator")]
    InvalidDiscriminator,

    /// Arithmetic overflow in AMM math.
    #[error("Math overflow in AMM calculation")]
    MathOverflow,

    /// One or both reserve values are zero — pool has no liquidity.
    #[error("Pool has zero liquidity")]
    ZeroLiquidity,

    /// Requested base_out is >= base_reserves (would drain the pool).
    #[error("Requested base_out ({requested}) would drain pool reserves ({reserves})")]
    InsufficientPoolLiquidity { requested: u64, reserves: u64 },

    /// Borsh / IO deserialization failure.
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] std::io::Error),
}

/// Convenience alias used throughout this crate.
pub type SwapResult<T> = Result<T, SwapError>;
