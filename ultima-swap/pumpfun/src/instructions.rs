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
//! Both `buy` and `sell` use the same 23-account layout derived from the
//! PumpSwap IDL.  If the program is upgraded and the ordering changes, update
//! the `account_metas` vectors in [`build_buy`] and [`build_sell`].
//!
//! After the IDL accounts both builders append the remaining accounts the
//! program reads by position: the `pool-v2` PDA when the pool has a coin
//! creator, then a buyback fee recipient and its quote-mint token account.
//! Cashback-coin pools also need the user's volume-accumulator accounts first;
//! [`Pool`] does not carry `is_cashback_coin` yet, so those are not added.

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
    /// Whether to track volume (0 = None/false).
    track_volume: u8,
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
    /// Token program that owns the pool's base mint.
    /// Determine at runtime via `getAccountInfo` on the mint.
    /// Use [`TOKEN_PROGRAM`] for legacy SPL tokens (and WSOL),
    /// [`TOKEN_2022_PROGRAM`] for Token Extensions.
    ///
    /// For pump.fun graduations the base is the token, usually Token-2022;
    /// for pools with WSOL as the base it is [`TOKEN_PROGRAM`].
    pub base_token_program: Pubkey,
    /// Token program that owns the pool's quote mint.
    ///
    /// For pump.fun graduations the quote is WSOL, so [`TOKEN_PROGRAM`];
    /// for pools with WSOL as the base it is the token's program.
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
    /// Token program that owns the pool's base mint (see
    /// [`BuyParams::base_token_program`]).
    pub base_token_program: Pubkey,
    /// Token program that owns the pool's quote mint (see
    /// [`BuyParams::quote_token_program`]).
    pub quote_token_program: Pubkey,
}

// ─────────────────────────────────────────────────────────────────────────────
// Instruction builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a PumpSwap `buy` [`Instruction`].
///
/// The instruction spends at most `max_quote_amount_in` quote atoms from the
/// user's quote ATA and transfers `base_amount_out` base-token atoms to the
/// user's base ATA.
///
/// # Prerequisite ATAs
///
/// The user must already have (or create in the same transaction, with
/// [`create_ata_if_needed`] and the mint's token program):
/// - A quote-mint ATA holding enough quote (for WSOL, wrapped lamports).
/// - A base-mint ATA.
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
        track_volume: 0,
    };
    let mut ix_data = Vec::with_capacity(25);
    data.serialize(&mut ix_data)
        .map_err(|e| SwapError::Deserialization(e.into()))?;

    // Base-mint accounts use the base program; every quote-mint account (the
    // user's, the fee recipient's, the creator vault's) uses the quote program.
    let user_base_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.base_mint, &params.base_token_program);
    let user_quote_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.quote_mint, &params.quote_token_program);
    let fee_recipient_quote_ata =
        get_associated_token_address_with_program(&fee_recipient, &params.pool_data.quote_mint, &params.quote_token_program);

    // Derive new PDAs for v2 accounts
    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", params.pool_data.coin_creator.as_ref()],
        &PUMP_AMM_PROGRAM_ID,
    );
    let coin_creator_vault_ata = get_associated_token_address_with_program(
        &coin_creator_vault_authority,
        &params.pool_data.quote_mint,
        &params.quote_token_program,
    );
    let (global_volume_accumulator, _) = Pubkey::find_program_address(
        &[b"global_volume_accumulator"],
        &PUMP_AMM_PROGRAM_ID,
    );
    let (user_volume_accumulator, _) = Pubkey::find_program_address(
        &[b"user_volume_accumulator", params.user.as_ref()],
        &PUMP_AMM_PROGRAM_ID,
    );
    let (fee_config, _) = Pubkey::find_program_address(
        &[b"fee_config", PUMP_AMM_PROGRAM_ID.as_ref()],
        &FEE_PROGRAM,
    );

    // Account ordering matches the PumpSwap IDL buy instruction (23 accounts).
    let mut accounts = vec![
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
        AccountMeta::new_readonly(params.base_token_program, false), // 11 base_token_program
        AccountMeta::new_readonly(params.quote_token_program, false), // 12 quote_token_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),       // 13 system_program
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false), // 14 associated_token_program
        AccountMeta::new_readonly(event_authority, false),      // 15 event_authority
        AccountMeta::new_readonly(PUMP_AMM_PROGRAM_ID, false),  // 16 program (self-CPI)
        AccountMeta::new(coin_creator_vault_ata, false),        // 17 coin_creator_vault_ata
        AccountMeta::new_readonly(coin_creator_vault_authority, false), // 18 coin_creator_vault_authority
        AccountMeta::new_readonly(global_volume_accumulator, false), // 19 global_volume_accumulator
        AccountMeta::new(user_volume_accumulator, false),       // 20 user_volume_accumulator
        AccountMeta::new_readonly(fee_config, false),           // 21 fee_config
        AccountMeta::new_readonly(FEE_PROGRAM, false),          // 22 fee_program
    ];
    accounts.extend(trade_remaining_accounts(
        &params.pool_data,
        params.fee_recipient_index,
        &params.quote_token_program,
    )?);

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

    // Base-mint accounts use the base program; every quote-mint account uses
    // the quote program.
    let user_base_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.base_mint, &params.base_token_program);
    let user_quote_ata =
        get_associated_token_address_with_program(&params.user, &params.pool_data.quote_mint, &params.quote_token_program);
    let fee_recipient_quote_ata =
        get_associated_token_address_with_program(&fee_recipient, &params.pool_data.quote_mint, &params.quote_token_program);

    // Derive PDAs for sell v2 accounts (21 total — no volume accumulators)
    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", params.pool_data.coin_creator.as_ref()],
        &PUMP_AMM_PROGRAM_ID,
    );
    let coin_creator_vault_ata = get_associated_token_address_with_program(
        &coin_creator_vault_authority,
        &params.pool_data.quote_mint,
        &params.quote_token_program,
    );
    let (fee_config, _) = Pubkey::find_program_address(
        &[b"fee_config", PUMP_AMM_PROGRAM_ID.as_ref()],
        &FEE_PROGRAM,
    );

    // Sell: 21 accounts (no global/user volume accumulators unlike buy's 23)
    let mut accounts = vec![
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
        AccountMeta::new_readonly(params.base_token_program, false), // 11 base_token_program
        AccountMeta::new_readonly(params.quote_token_program, false), // 12 quote_token_program
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),       // 13 system_program
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false), // 14 associated_token_program
        AccountMeta::new_readonly(event_authority, false),      // 15 event_authority
        AccountMeta::new_readonly(PUMP_AMM_PROGRAM_ID, false),  // 16 program (self-CPI)
        AccountMeta::new(coin_creator_vault_ata, false),        // 17 coin_creator_vault_ata
        AccountMeta::new_readonly(coin_creator_vault_authority, false), // 18 coin_creator_vault_authority
        AccountMeta::new_readonly(fee_config, false),           // 19 fee_config
        AccountMeta::new_readonly(FEE_PROGRAM, false),          // 20 fee_program
    ];
    accounts.extend(trade_remaining_accounts(
        &params.pool_data,
        params.fee_recipient_index,
        &params.quote_token_program,
    )?);

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
///
/// Fields keep PumpSwap's own base/quote naming. For a pump.fun graduation
/// the base is the token and the quote WSOL; pools made by calling
/// `create_pool` directly are often the other way round. Use
/// [`CreatePoolDetected::token_mint`] for the non-WSOL side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePoolDetected {
    /// The newly-created pool address (account 0).
    pub pool: Pubkey,
    /// The signer that created the pool (account 2). For a graduation this is
    /// pump.fun's migration authority, not the token's deployer: see
    /// [`CreatePoolDetected::coin_creator`].
    pub creator: Pubkey,
    /// The pool's base mint (account 3).
    pub base_mint: Pubkey,
    /// The pool's quote mint (account 4).
    pub quote_mint: Pubkey,
    /// The pool's LP mint (account 5).
    pub lp_mint: Pubkey,
    /// The pool's base vault (account 9).
    pub pool_base_token_account: Pubkey,
    /// The pool's quote vault (account 10).
    pub pool_quote_token_account: Pubkey,
    /// Token program of the base mint (account 13).
    pub base_token_program: Pubkey,
    /// Token program of the quote mint (account 14).
    pub quote_token_program: Pubkey,
    /// `index` argument.
    pub index: u16,
    /// `base_amount_in` argument: the pool's opening base reserves.
    pub base_amount_in: u64,
    /// `quote_amount_in` argument: the pool's opening quote reserves.
    pub quote_amount_in: u64,
    /// `coin_creator` argument: the token's deployer for a graduation.
    pub coin_creator: Pubkey,
    /// `is_mayhem_mode` argument.
    pub is_mayhem_mode: bool,
}

impl CreatePoolDetected {
    /// The mint on the other side of WSOL, with its token program, or `None`
    /// if neither or both sides are WSOL.
    pub fn token_mint(&self) -> Option<(Pubkey, Pubkey)> {
        match (self.base_mint == WSOL_MINT, self.quote_mint == WSOL_MINT) {
            (true, false) => Some((self.quote_mint, self.quote_token_program)),
            (false, true) => Some((self.base_mint, self.base_token_program)),
            _ => None,
        }
    }
}

/// Attempt to parse a `create_pool` instruction from raw instruction bytes and
/// account keys.
///
/// Returns `None` if the discriminator doesn't match, the arguments are
/// short, there are too few accounts, or account 1 is not PumpSwap's
/// [`GLOBAL_CONFIG`].
///
/// # Account ordering for create_pool
///
/// Verified against on-chain create_pool transactions (2026-11-03):
///
/// ```text
/// 0  pool              (new pool address)
/// 1  global_config
/// 2  creator           (signer)
/// 3  base_mint         (the token, for pump.fun graduations)
/// 4  quote_mint        (WSOL, for pump.fun graduations)
/// 5  lp_mint
/// 6  user_base_token_account
/// 7  user_quote_token_account
/// 8  user_pool_token_account
/// 9  pool_base_token_account
/// 10 pool_quote_token_account
/// 11 system_program
/// 12 token_2022_program
/// 13 base_token_program
/// 14 quote_token_program
/// 15 associated_token_program
/// 16 event_authority
/// 17 program
/// ```
///
/// # Arguments (after the 8-byte discriminator)
///
/// ```text
/// index           u16
/// base_amount_in  u64
/// quote_amount_in u64
/// coin_creator    Pubkey
/// is_mayhem_mode  bool
/// ...             (a further flag byte, not read)
/// ```
pub fn try_parse_create_pool(
    ix_data: &[u8],
    account_keys: &[Pubkey],
) -> Option<CreatePoolDetected> {
    const ARGS_LEN: usize = 2 + 8 + 8 + 32 + 1;
    if ix_data.len() < 8 + ARGS_LEN {
        return None;
    }
    let disc: [u8; 8] = ix_data[..8].try_into().ok()?;
    if disc != CREATE_POOL_DISCRIMINATOR {
        return None;
    }
    if account_keys.len() < 15 || account_keys[1] != GLOBAL_CONFIG {
        return None;
    }
    let args = &ix_data[8..];
    Some(CreatePoolDetected {
        pool: account_keys[0],
        creator: account_keys[2],
        base_mint: account_keys[3],
        quote_mint: account_keys[4],
        lp_mint: account_keys[5],
        pool_base_token_account: account_keys[9],
        pool_quote_token_account: account_keys[10],
        base_token_program: account_keys[13],
        quote_token_program: account_keys[14],
        index: u16::from_le_bytes(args[0..2].try_into().ok()?),
        base_amount_in: u64::from_le_bytes(args[2..10].try_into().ok()?),
        quote_amount_in: u64::from_le_bytes(args[10..18].try_into().ok()?),
        coin_creator: Pubkey::new_from_array(args[18..50].try_into().ok()?),
        is_mayhem_mode: args[50] != 0,
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

/// Remaining accounts of `buy` and `sell`, in the order the program reads them
/// (pump-swap-sdk `buyInstructionsNoPool` / `sellInstructionsNoPool`): the
/// `pool-v2` PDA when the pool has a coin creator (else `InvalidPoolV2`, 6062),
/// then a buyback fee recipient and its quote-mint token account, writable
/// (else `BuybackFeeRecipientMissing`, 6058).
fn trade_remaining_accounts(
    pool_data: &Pool,
    fee_recipient_index: usize,
    quote_token_program: &Pubkey,
) -> SwapResult<Vec<AccountMeta>> {
    let mut accounts = Vec::with_capacity(3);
    if pool_data.coin_creator != Pubkey::default() {
        accounts.push(AccountMeta::new_readonly(
            derive_pool_v2(&pool_data.base_mint).0,
            false,
        ));
    }
    accounts.push(AccountMeta::new_readonly(
        buyback_fee_recipient(fee_recipient_index)?,
        false,
    ));
    accounts.push(AccountMeta::new(
        buyback_fee_recipient_token_account(
            fee_recipient_index,
            &pool_data.quote_mint,
            quote_token_program,
        )?,
        false,
    ));
    Ok(accounts)
}

/// The `pool-v2` PDA of a base mint: seeds `["pool-v2", base_mint]`.
pub fn derive_pool_v2(base_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_V2_SEED, base_mint.as_ref()], &PUMP_AMM_PROGRAM_ID)
}

/// Token account for `quote_mint` of the buyback fee recipient that a trade
/// built with `fee_recipient_index` pays: the recipient's ATA under the quote
/// mint's token program.
pub fn buyback_fee_recipient_token_account(
    fee_recipient_index: usize,
    quote_mint: &Pubkey,
    quote_token_program: &Pubkey,
) -> SwapResult<Pubkey> {
    Ok(get_associated_token_address_with_program(
        &buyback_fee_recipient(fee_recipient_index)?,
        quote_mint,
        quote_token_program,
    ))
}

fn buyback_fee_recipient(index: usize) -> SwapResult<Pubkey> {
    BUYBACK_FEE_RECIPIENTS
        .get(index)
        .copied()
        .ok_or(SwapError::MathOverflow) // same out-of-range error as fee_recipient
}

fn fee_recipient(index: usize) -> SwapResult<Pubkey> {
    PROTOCOL_FEE_RECIPIENTS
        .get(index)
        .copied()
        .ok_or_else(|| SwapError::MathOverflow) // reuse closest error; index OOB
}

/// Derive the Associated Token Account (ATA) address: PDA of
/// [wallet, token_program, mint] under the ATA program.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(coin_creator: Pubkey) -> Pool {
        Pool {
            pool_bump: 255,
            index: 0,
            creator: Pubkey::new_unique(),
            base_mint: WSOL_MINT,
            quote_mint: Pubkey::new_unique(),
            lp_mint: Pubkey::new_unique(),
            pool_base_token_account: Pubkey::new_unique(),
            pool_quote_token_account: Pubkey::new_unique(),
            lp_supply: 0,
            is_mayhem_mode: false,
            coin_creator,
        }
    }

    fn build(pool_data: &Pool) -> (Instruction, Instruction) {
        let pool = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let buy = build_buy(BuyParams {
            pool,
            pool_data: pool_data.clone(),
            user,
            base_amount_out: 1,
            max_quote_amount_in: 1,
            fee_recipient_index: 2,
            base_token_program: TOKEN_PROGRAM,
            quote_token_program: TOKEN_2022_PROGRAM,
        })
        .unwrap();
        let sell = build_sell(SellParams {
            pool,
            pool_data: pool_data.clone(),
            user,
            base_amount_in: 1,
            min_quote_amount_out: 0,
            fee_recipient_index: 2,
            base_token_program: TOKEN_PROGRAM,
            quote_token_program: TOKEN_2022_PROGRAM,
        })
        .unwrap();
        (buy, sell)
    }

    fn keys(ix: &Instruction) -> Vec<Pubkey> {
        ix.accounts.iter().map(|m| m.pubkey).collect()
    }

    #[test]
    fn buyback_pair_follows_the_idl_accounts() {
        let pool_data = pool(Pubkey::default());
        let (buy, sell) = build(&pool_data);
        let expected = [
            BUYBACK_FEE_RECIPIENTS[2],
            get_associated_token_address_with_program(
                &BUYBACK_FEE_RECIPIENTS[2],
                &pool_data.quote_mint,
                &TOKEN_2022_PROGRAM,
            ),
        ];
        assert_eq!(&keys(&buy)[23..], &expected);
        assert_eq!(&keys(&sell)[21..], &expected);
        for ix in [&buy, &sell] {
            let n = ix.accounts.len();
            assert!(!ix.accounts[n - 2].is_writable && ix.accounts[n - 1].is_writable);
        }
    }

    #[test]
    fn pool_v2_comes_first_when_the_pool_has_a_coin_creator() {
        let pool_data = pool(Pubkey::new_unique());
        let (buy, sell) = build(&pool_data);
        let pool_v2 = derive_pool_v2(&pool_data.base_mint).0;
        assert_eq!(buy.accounts.len(), 26);
        assert_eq!(sell.accounts.len(), 24);
        assert_eq!(keys(&buy)[23], pool_v2);
        assert_eq!(keys(&sell)[21], pool_v2);
    }

    fn k(s: &str) -> Pubkey {
        s.parse().unwrap()
    }

    /// The `create_pool` pump.fun's migration called in slot 464874772, for
    /// pool F6KbJYYx...: index 0, 206.9M tokens and 84.99 SOL in.
    fn graduation_create_pool() -> (Vec<u8>, Vec<Pubkey>) {
        let mut data = CREATE_POOL_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&206_900_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&84_990_359_056u64.to_le_bytes());
        data.extend_from_slice(k("C8oBfdKGaaNeNVqzmL2awkrTWxdjGd6Ho7qssqSCS1Yx").as_ref());
        data.extend_from_slice(&[0; 2]);
        let accounts = [
            "F6KbJYYxzdT27XZD3mznj9PCqUG5kojHQFZzQFE13rPp",
            "ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw",
            "3rmiDHgHJJjusvK7SoZrg1kFmuRY8eZtKAtnRessqYx5",
            "8V16miNE7UHGUGD4cWP9MX8SNSJK1fhaxqTasHCJpump",
            "So11111111111111111111111111111111111111112",
            "GxXDTiGZjqTw7xK4jJfQFiTBXkeyvFQ3jpp83PLBmVAi",
            "RvKZ3wSvFgvDBv97cGPgkVhyFPJeDrpBhyKtzdESx5y",
            "Deu985QqBpfUuSjwMoV2cHRgwdrDHDyegsajBVYwruk",
            "2B8awmPxSALJGRBWiU4T1tPWWkZY6GgdH8qX8ATayr34",
            "EmZZ5bNaJ9qLG5MoneCuG1b5kEaj15dXscTRRLpWibC",
            "4pAzP13YwoV4zNZEyHfa8GwGphVJPjcdxzCnLgXCphVV",
            "11111111111111111111111111111111",
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
            "GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR",
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
        ]
        .iter()
        .map(|s| k(s))
        .collect();
        (data, accounts)
    }

    #[test]
    fn a_graduation_create_pool_is_read_with_the_token_as_base() {
        let (data, accounts) = graduation_create_pool();
        let p = try_parse_create_pool(&data, &accounts).unwrap();
        assert_eq!(p.pool, accounts[0]);
        // Account 1 is the global config, not the creator.
        assert_eq!(p.creator, k("3rmiDHgHJJjusvK7SoZrg1kFmuRY8eZtKAtnRessqYx5"));
        assert_eq!(p.base_mint, k("8V16miNE7UHGUGD4cWP9MX8SNSJK1fhaxqTasHCJpump"));
        assert_eq!(p.quote_mint, WSOL_MINT);
        assert_eq!(p.lp_mint, accounts[5]);
        assert_eq!(p.pool_base_token_account, accounts[9]);
        assert_eq!(p.pool_quote_token_account, accounts[10]);
        assert_eq!(p.base_token_program, TOKEN_2022_PROGRAM);
        assert_eq!(p.quote_token_program, TOKEN_PROGRAM);
        assert_eq!(p.index, 0);
        assert_eq!(p.base_amount_in, 206_900_000_000_000);
        assert_eq!(p.quote_amount_in, 84_990_359_056);
        assert_eq!(p.coin_creator, k("C8oBfdKGaaNeNVqzmL2awkrTWxdjGd6Ho7qssqSCS1Yx"));
        assert!(!p.is_mayhem_mode);
        assert_eq!(p.token_mint(), Some((p.base_mint, TOKEN_2022_PROGRAM)));
    }

    #[test]
    fn create_pool_is_rejected_without_its_args_or_the_global_config() {
        let (data, accounts) = graduation_create_pool();
        assert!(try_parse_create_pool(&data[..8 + 50], &accounts).is_none());
        assert!(try_parse_create_pool(&data, &accounts[..14]).is_none());
        let mut wrong = accounts.clone();
        wrong[1] = Pubkey::new_unique();
        assert!(try_parse_create_pool(&data, &wrong).is_none());
    }

    #[test]
    fn a_wsol_base_create_pool_finds_the_token_on_the_quote_side() {
        let (data, mut accounts) = graduation_create_pool();
        accounts.swap(3, 4);
        accounts.swap(13, 14);
        let p = try_parse_create_pool(&data, &accounts).unwrap();
        assert_eq!(p.base_mint, WSOL_MINT);
        assert_eq!(p.token_mint(), Some((accounts[4], TOKEN_2022_PROGRAM)));
    }

    /// Buy and sell on the graduated pool F6KbJYYx..., a Token-2022 token with
    /// a coin creator, as simulated on mainnet.
    #[test]
    fn a_graduation_trades_its_token_2022_base_and_wsol_quote() {
        let (data, accounts) = graduation_create_pool();
        let p = try_parse_create_pool(&data, &accounts).unwrap();
        let pool_data = Pool {
            pool_bump: 255,
            index: p.index,
            creator: p.creator,
            base_mint: p.base_mint,
            quote_mint: p.quote_mint,
            lp_mint: p.lp_mint,
            pool_base_token_account: p.pool_base_token_account,
            pool_quote_token_account: p.pool_quote_token_account,
            lp_supply: 0,
            coin_creator: p.coin_creator,
            is_mayhem_mode: false,
        };
        let user = Pubkey::new_unique();
        let buy = build_buy(BuyParams {
            pool: p.pool,
            pool_data: pool_data.clone(),
            user,
            base_amount_out: 10,
            max_quote_amount_in: 1_000,
            fee_recipient_index: 0,
            base_token_program: TOKEN_2022_PROGRAM,
            quote_token_program: TOKEN_PROGRAM,
        })
        .unwrap();
        let sell = build_sell(SellParams {
            pool: p.pool,
            pool_data: pool_data.clone(),
            user,
            base_amount_in: 10,
            min_quote_amount_out: 1,
            fee_recipient_index: 0,
            base_token_program: TOKEN_2022_PROGRAM,
            quote_token_program: TOKEN_PROGRAM,
        })
        .unwrap();
        let wsol_ata = |owner: &Pubkey| get_associated_token_address_with_program(owner, &WSOL_MINT, &TOKEN_PROGRAM);
        let creator_vault = Pubkey::find_program_address(
            &[b"creator_vault", p.coin_creator.as_ref()],
            &PUMP_AMM_PROGRAM_ID,
        )
        .0;
        assert_eq!(buy.accounts.len(), 26);
        assert_eq!(sell.accounts.len(), 24);
        for ix in [&buy, &sell] {
            let keys = keys(ix);
            let n = keys.len();
            assert_eq!(
                keys[5],
                get_associated_token_address_with_program(&user, &p.base_mint, &TOKEN_2022_PROGRAM)
            );
            assert_eq!(keys[6], wsol_ata(&user));
            assert_eq!(keys[10], wsol_ata(&PROTOCOL_FEE_RECIPIENTS[0]));
            assert_eq!(keys[11], TOKEN_2022_PROGRAM);
            assert_eq!(keys[12], TOKEN_PROGRAM);
            assert_eq!(keys[17], wsol_ata(&creator_vault));
            assert_eq!(keys[18], creator_vault);
            assert_eq!(keys[n - 3], derive_pool_v2(&p.base_mint).0);
            assert_eq!(keys[n - 2], BUYBACK_FEE_RECIPIENTS[0]);
            assert_eq!(keys[n - 1], wsol_ata(&BUYBACK_FEE_RECIPIENTS[0]));
        }
    }

    #[test]
    fn a_wsol_base_pool_keeps_its_accounts_on_token_program() {
        let pool_data = pool(Pubkey::default());
        let (buy, sell) = build(&pool_data);
        for ix in [&buy, &sell] {
            assert_eq!(
                ix.accounts[5].pubkey,
                get_associated_token_address_with_program(&ix.accounts[1].pubkey, &WSOL_MINT, &TOKEN_PROGRAM)
            );
            assert_eq!(ix.accounts[11].pubkey, TOKEN_PROGRAM);
            assert_eq!(ix.accounts[12].pubkey, TOKEN_2022_PROGRAM);
        }
    }
}
