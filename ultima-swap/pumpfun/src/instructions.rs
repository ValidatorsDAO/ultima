//! Instruction builders for PumpSwap AMM.
//!
//! # Usage
//!
//! ```rust,no_run
//! use ultima_swap_pumpfun::{
//!     accounts::Pool,
//!     constants::WSOL_MINT,
//!     instructions::{build_buy, build_sell, BuyParams, SellParams},
//!     math::{quote_in_for_exact_base_out, with_slippage_max, DEFAULT_FEE_BPS},
//! };
//! use solana_pubkey::Pubkey;
//!
//! // Assume `pool` was fetched from on-chain and `user` is the signer.
//! // let pool: Pool = ...;
//! // let user: Pubkey = ...;
//! // let pool_address: Pubkey = ...;
//! ```
//!
//! # Account ordering
//!
//! Both `buy` and `sell` use the same 17-account layout derived from the
//! PumpSwap IDL.  If the program is upgraded and the ordering changes, update
//! the `account_metas` vectors in [`build_buy`] and [`build_sell`].

use borsh::BorshSerialize;
use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};

use crate::{
    accounts::{derive_event_authority, Pool},
    constants::*,
    error::{SwapError, SwapResult},
};

// ─────────────────────────────────────────────────────────────────────────────
// Instruction data types
// ─────────────────────────────────────────────────────────────────────────────

/// On-wire data for the `buy` instruction.
#[derive(Debug, Clone, BorshSerialize)]
struct BuyInstructionData {
    /// Anchor discriminator.
    discriminator: [u8; 8],
    /// Number of base-token atoms to receive.
    base_amount_out: u64,
    /// Maximum lamports the user is willing to spend (slippage guard).
    max_quote_amount_in: u64,
}

/// On-wire data for the `sell` instruction.
#[derive(Debug, Clone, BorshSerialize)]
struct SellInstructionData {
    /// Anchor discriminator.
    discriminator: [u8; 8],
    /// Number of base-token atoms to sell.
    base_amount_in: u64,
    /// Minimum lamports the user requires to receive (slippage guard).
    min_quote_amount_out: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public parameter structs
// ─────────────────────────────────────────────────────────────────────────────

/// Parameters for a PumpSwap **buy** (receive base, spend quote/SOL).
#[derive(Debug, Clone)]
pub struct BuyParams {
    /// Address of the Pool account.
    pub pool: Pubkey,
    /// Deserialized pool data (needed to look up mints and vault addresses).
    pub pool_data: Pool,
    /// Wallet/signer that pays SOL and receives base tokens.
    pub user: Pubkey,
    /// Exact amount of base-token atoms to receive.
    pub base_amount_out: u64,
    /// Maximum SOL (lamports) willing to spend, including slippage buffer.
    pub max_quote_amount_in: u64,
    /// Which protocol-fee recipient to use (index 0–7 into
    /// [`PROTOCOL_FEE_RECIPIENTS`]).  Callers can rotate this for throughput.
    pub fee_recipient_index: usize,
    /// Token program that owns the graduated (quote) mint.
    /// Determine at runtime via `getAccountInfo` on the mint.
    /// Use [`TOKEN_PROGRAM`] for legacy SPL tokens, [`TOKEN_2022_PROGRAM`] for Token Extensions.
    pub quote_token_program: Pubkey,
}

/// Parameters for a PumpSwap **sell** (spend base, receive quote/SOL).
#[derive(Debug, Clone)]
pub struct SellParams {
    /// Address of the Pool account.
    pub pool: Pubkey,
    /// Deserialized pool data.
    pub pool_data: Pool,
    /// Wallet/signer that spends base tokens and receives SOL.
    pub user: Pubkey,
    /// Exact amount of base-token atoms to sell.
    pub base_amount_in: u64,
    /// Minimum SOL (lamports) that must be received; tx aborts if not met.
    pub min_quote_amount_out: u64,
    /// Protocol-fee recipient index (0–7).
    pub fee_recipient_index: usize,
    /// Token program that owns the graduated (quote) mint.
    pub quote_token_program: Pubkey,
}

// ─────────────────────────────────────────────────────────────────────────────
// Instruction builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a PumpSwap `buy` [`Instruction`].
///
/// The instruction pays `max_quote_amount_in` lamports from `user` and
/// transfers `base_amount_out` base-token atoms to the user's ATA.
///
/// # Prerequisite ATAs
///
/// The user must already have (or create in the same transaction):
/// - A WSOL ATA with enough lamports.
/// - A base-mint ATA (create with [`create_base_ata_if_needed`] if necessary).
///
/// # Errors
///
/// Returns [`SwapError`] if the fee-recipient index is out of range or
/// Borsh serialization fails (should never happen).
pub fn build_buy(params: BuyParams) -> SwapResult<Instruction> {
    let fee_recipient = fee_recipient(params.fee_recipient_index)?;
    let (event_authority, _) = derive_event_authority();

    let data = BuyInstructionData {
        discriminator: BUY_DISCRIMINATOR,
        base_amount_out: params.base_amount_out,
        max_quote_amount_in: params.max_quote_amount_in,
    };
    let mut ix_data = Vec::with_capacity(24);
    data.serialize(&mut ix_data)
        .map_err(|e| SwapError::Deserialization(e.into()))?;

    // PumpSwap naming: base_mint = WSOL (always TOKEN_PROGRAM),
    // quote_mint = graduated token (TOKEN_PROGRAM or TOKEN_2022 depending on mint).
    let user_base_ata =
        get_associated_token_address(&params.user, &params.pool_data.base_mint); // WSOL → TOKEN_PROGRAM
    let user_quote_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.quote_mint, &params.quote_token_program);
    let fee_recipient_quote_ata =
        get_associated_token_address_with_program(&fee_recipient, &params.pool_data.quote_mint, &params.quote_token_program);

    // Account ordering matches the PumpSwap IDL buy instruction.
    let accounts = vec![
        AccountMeta::new(params.pool, false),                   // 0  pool
        AccountMeta::new(params.user, true),                    // 1  user (signer)
        AccountMeta::new_readonly(GLOBAL_CONFIG, false),        // 2  global_config
        AccountMeta::new_readonly(params.pool_data.base_mint, false), // 3  base_mint
        AccountMeta::new_readonly(params.pool_data.quote_mint, false), // 4  quote_mint
        AccountMeta::new(user_base_ata, false),                 // 5  user_base_token_account
        AccountMeta::new(user_quote_ata, false),                // 6  user_quote_token_account
        AccountMeta::new(params.pool_data.pool_base_token_account, false), // 7  pool_base_token_account
        AccountMeta::new(params.pool_data.pool_quote_token_account, false), // 8  pool_quote_token_account
        AccountMeta::new(fee_recipient, false),                 // 9  protocol_fee_recipient
        AccountMeta::new(fee_recipient_quote_ata, false),       // 10 protocol_fee_recipient_token_account
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),        // 11 token_program
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),   // 12 token_2022_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),       // 13 system_program
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false), // 14 associated_token_program
        AccountMeta::new_readonly(event_authority, false),      // 15 event_authority
        AccountMeta::new_readonly(PUMP_AMM_PROGRAM_ID, false),  // 16 program (self-CPI)
    ];

    Ok(Instruction {
        program_id: PUMP_AMM_PROGRAM_ID,
        accounts,
        data: ix_data,
    })
}

/// Build a PumpSwap `sell` [`Instruction`].
///
/// Transfers `base_amount_in` base-token atoms from the user's ATA and
/// credits at least `min_quote_amount_out` lamports to the user.
pub fn build_sell(params: SellParams) -> SwapResult<Instruction> {
    let fee_recipient = fee_recipient(params.fee_recipient_index)?;
    let (event_authority, _) = derive_event_authority();

    let data = SellInstructionData {
        discriminator: SELL_DISCRIMINATOR,
        base_amount_in: params.base_amount_in,
        min_quote_amount_out: params.min_quote_amount_out,
    };
    let mut ix_data = Vec::with_capacity(24);
    data.serialize(&mut ix_data)
        .map_err(|e| SwapError::Deserialization(e.into()))?;

    // PumpSwap naming: base_mint = WSOL (always TOKEN_PROGRAM),
    // quote_mint = graduated token (runtime-determined token program).
    let user_base_ata =
        get_associated_token_address(&params.user, &params.pool_data.base_mint); // WSOL → TOKEN_PROGRAM
    let user_quote_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.quote_mint, &params.quote_token_program);
    let fee_recipient_quote_ata =
        get_associated_token_address_with_program(&fee_recipient, &params.pool_data.quote_mint, &params.quote_token_program);

    let accounts = vec![
        AccountMeta::new(params.pool, false),                   // 0  pool
        AccountMeta::new(params.user, true),                    // 1  user (signer)
        AccountMeta::new_readonly(GLOBAL_CONFIG, false),        // 2  global_config
        AccountMeta::new_readonly(params.pool_data.base_mint, false), // 3  base_mint
        AccountMeta::new_readonly(params.pool_data.quote_mint, false), // 4  quote_mint
        AccountMeta::new(user_base_ata, false),                 // 5  user_base_token_account
        AccountMeta::new(user_quote_ata, false),                // 6  user_quote_token_account
        AccountMeta::new(params.pool_data.pool_base_token_account, false), // 7  pool_base_token_account
        AccountMeta::new(params.pool_data.pool_quote_token_account, false), // 8  pool_quote_token_account
        AccountMeta::new(fee_recipient, false),                 // 9  protocol_fee_recipient
        AccountMeta::new(fee_recipient_quote_ata, false),       // 10 protocol_fee_recipient_token_account
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),        // 11 token_program
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM, false),   // 12 token_2022_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),       // 13 system_program
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false), // 14 associated_token_program
        AccountMeta::new_readonly(event_authority, false),      // 15 event_authority
        AccountMeta::new_readonly(PUMP_AMM_PROGRAM_ID, false),  // 16 program (self-CPI)
    ];

    Ok(Instruction {
        program_id: PUMP_AMM_PROGRAM_ID,
        accounts,
        data: ix_data,
    })
}

/// Build an ATA-creation instruction for the base mint if it doesn't yet exist.
///
/// Include this before the buy instruction when the user's base ATA may not
/// exist (e.g., first purchase of this token).
/// Build an idempotent ATA-creation instruction for the base mint.
///
/// Uses instruction index 1 of the ATA program (CreateIdempotent).
/// Create an ATA for a base mint.  Pump.fun graduated tokens use Token-2022,
/// so we must pass the correct token program.  `token_program` should be
/// [`TOKEN_2022_PROGRAM`] for graduated base mints and [`TOKEN_PROGRAM`] for WSOL.
/// Create an ATA for the graduated token (PumpSwap's quote mint).
///
/// `token_program` must match the graduated token's owner program
/// (TOKEN_PROGRAM for legacy SPL tokens, TOKEN_2022_PROGRAM for Token Extensions).
/// Determine this at runtime by inspecting the mint account's `owner` field.
pub fn create_quote_ata_if_needed(user: &Pubkey, quote_mint: &Pubkey, token_program: &Pubkey) -> Instruction {
    create_ata_if_needed(user, quote_mint, token_program)
}

/// Backwards-compat alias. Prefer [`create_quote_ata_if_needed`].
pub fn create_base_ata_if_needed(user: &Pubkey, base_mint: &Pubkey) -> Instruction {
    // Legacy: assumes TOKEN_PROGRAM. Callers should migrate to create_quote_ata_if_needed.
    create_ata_if_needed(user, base_mint, &TOKEN_PROGRAM)
}

/// Create an ATA for any mint with an explicit token program.
pub fn create_ata_if_needed(user: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Instruction {
    let ata = get_associated_token_address_with_program(user, mint, token_program);
    Instruction {
        program_id: ASSOCIATED_TOKEN_PROGRAM,
        accounts: vec![
            AccountMeta::new(*user, true),                     // payer
            AccountMeta::new(ata, false),                      // associated token account
            AccountMeta::new_readonly(*user, false),           // wallet
            AccountMeta::new_readonly(*mint, false),           // mint
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),  // system program
            AccountMeta::new_readonly(*token_program, false),  // token program
        ],
        data: vec![1], // CreateIdempotent instruction discriminator
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// create_pool detection
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed representation of a `create_pool` instruction found in a transaction.
///
/// This is used for **detection/monitoring** — subscribing to program logs and
/// extracting newly-created pools — not for building create_pool instructions
/// (which is handled server-side by Pump.fun's migration service).
#[derive(Debug, Clone)]
pub struct CreatePoolDetected {
    /// The newly-created pool address (account key at index 0).
    pub pool: Pubkey,
    /// The pool creator.
    pub creator: Pubkey,
    /// Base (graduated token) mint.
    pub base_mint: Pubkey,
    /// Quote mint (typically WSOL).
    pub quote_mint: Pubkey,
}

/// Attempt to parse a `create_pool` instruction from raw instruction bytes and
/// account keys.
///
/// Returns `None` if the discriminator doesn't match or there are too few
/// accounts.
///
/// # Account ordering for create_pool
///
/// Verified against on-chain create_pool transactions (2026-04-03):
///
/// ```text
/// 0  pool              (new pool address)
/// 1  creator           (payer / signer)
/// 2  global_config
/// 3  base_mint         (WSOL — PumpSwap calls WSOL "base")
/// 4  quote_mint        (graduated token — PumpSwap calls this "quote")
/// 5  lp_mint
/// 6  pool_base_vault   (WSOL vault)
/// 7  pool_quote_vault  (graduated token vault)
/// ...
/// ```
///
/// **IMPORTANT:** PumpSwap's naming is inverted from the usual convention.
/// On-chain Pool struct has `base_mint = WSOL` and `quote_mint = graduated`.
/// The `base_mint` field in [`CreatePoolDetected`] returns the **graduated
/// token** (index 4) for downstream convenience — the thing we want to trade.
pub fn try_parse_create_pool(
    ix_data: &[u8],
    account_keys: &[Pubkey],
) -> Option<CreatePoolDetected> {
    if ix_data.len() < 8 {
        return None;
    }
    let disc: [u8; 8] = ix_data[..8].try_into().ok()?;
    if disc != CREATE_POOL_DISCRIMINATOR {
        return None;
    }
    if account_keys.len() < 6 {
        return None;
    }
    // Return graduated token as "base_mint" for downstream (it's actually
    // PumpSwap's quote_mint at index 4).
    Some(CreatePoolDetected {
        pool: account_keys[0],
        creator: account_keys[1],
        base_mint: account_keys[4],  // graduated token (PumpSwap's quote_mint)
        quote_mint: account_keys[3], // WSOL (PumpSwap's base_mint)
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// swap (buy/sell) detection
// ─────────────────────────────────────────────────────────────────────────────

/// Direction of a detected swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDirection {
    Buy,
    Sell,
}

/// Parsed representation of a buy or sell instruction found in a transaction.
#[derive(Debug, Clone)]
pub struct SwapDetected {
    pub direction: SwapDirection,
    /// Pool address (account key at index 0).
    pub pool: Pubkey,
    /// User/signer (account key at index 1).
    pub user: Pubkey,
    /// Base mint (account key at index 2).
    pub base_mint: Pubkey,
    /// For buy: base_amount_out (tokens received).
    /// For sell: base_amount_in (tokens sold).
    pub base_amount: u64,
    /// For buy: max_quote_amount_in (max SOL spent).
    /// For sell: min_quote_amount_out (min SOL received).
    pub quote_amount: u64,
}

/// Attempt to parse a buy or sell instruction from raw instruction bytes
/// and resolved account keys.
///
/// Returns `None` if the discriminator doesn't match buy or sell, or if
/// there are too few accounts/bytes.
///
/// # Buy account layout
/// ```text
/// 0  pool
/// 1  user (signer)
/// 2  base_mint
/// ...
/// ```
///
/// # Instruction data layout (both buy & sell)
/// ```text
/// [0..8]   discriminator
/// [8..16]  amount_1 (u64 LE) — buy: base_amount_out, sell: base_amount_in
/// [16..24] amount_2 (u64 LE) — buy: max_quote_in, sell: min_quote_out
/// ```
pub fn try_parse_swap(
    ix_data: &[u8],
    account_keys: &[Pubkey],
) -> Option<SwapDetected> {
    if ix_data.len() < 24 || account_keys.len() < 3 {
        return None;
    }
    let disc: [u8; 8] = ix_data[..8].try_into().ok()?;
    let direction = if disc == BUY_DISCRIMINATOR {
        SwapDirection::Buy
    } else if disc == SELL_DISCRIMINATOR {
        SwapDirection::Sell
    } else {
        return None;
    };
    let base_amount = u64::from_le_bytes(ix_data[8..16].try_into().ok()?);
    let quote_amount = u64::from_le_bytes(ix_data[16..24].try_into().ok()?);
    Some(SwapDetected {
        direction,
        pool: account_keys[0],
        user: account_keys[1],
        base_mint: account_keys[2],
        quote_amount,
        base_amount,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn fee_recipient(index: usize) -> SwapResult<Pubkey> {
    PROTOCOL_FEE_RECIPIENTS
        .get(index)
        .copied()
        .ok_or_else(|| SwapError::MathOverflow) // reuse closest error; index OOB
}

/// Derive the Associated Token Account (ATA) address.
/// This is the standard ATA derivation: PDA of [wallet, TOKEN_PROGRAM, mint] under the ATA program.
fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program(wallet, mint, &TOKEN_PROGRAM)
}

fn get_associated_token_address_with_program(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program.as_ref(),
            mint.as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM,
    ).0
}
