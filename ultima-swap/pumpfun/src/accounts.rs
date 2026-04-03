use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;

use crate::constants::*;

// ─────────────────────────────────────────────────────────────────────────────
// Pool
// ─────────────────────────────────────────────────────────────────────────────

/// On-chain Pool account.
///
/// Layout (after 8-byte discriminator):
/// ```text
///  pool_bump               u8
///  index                   u16
///  creator                 Pubkey
///  base_mint               Pubkey  (graduated token)
///  quote_mint              Pubkey  (usually WSOL)
///  lp_mint                 Pubkey
///  pool_base_token_account Pubkey
///  pool_quote_token_account Pubkey
///  lp_supply               u64
///  is_mayhem_mode          bool
/// ```
#[derive(Debug, Clone, BorshDeserialize, BorshSerialize)]
pub struct Pool {
    pub pool_bump: u8,
    pub index: u16,
    pub creator: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub pool_base_token_account: Pubkey,
    pub pool_quote_token_account: Pubkey,
    pub lp_supply: u64,
    pub is_mayhem_mode: bool,
}

/// Anchor account discriminator for Pool (sha256("account:Pool")[..8]).
pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

impl Pool {
    /// Deserialize from raw account data (skips the 8-byte discriminator).
    pub fn try_from_slice(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Account data too short",
            ));
        }
        let disc = &data[..8];
        if disc != POOL_DISCRIMINATOR {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Pool discriminator",
            ));
        }
        BorshDeserialize::try_from_slice(&data[8..])
    }

    /// Derive the Pool PDA.
    ///
    /// Seeds: `["pool", index.to_le_bytes(), creator, base_mint, quote_mint]`
    pub fn derive_pda(
        index: u16,
        creator: &Pubkey,
        base_mint: &Pubkey,
        quote_mint: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                POOL_SEED,
                &index.to_le_bytes(),
                creator.as_ref(),
                base_mint.as_ref(),
                quote_mint.as_ref(),
            ],
            &PUMP_AMM_PROGRAM_ID,
        )
    }

    /// Derive the LP mint PDA for this pool.
    ///
    /// Seeds: `["pool_lp_mint", pool_address]`
    pub fn derive_lp_mint(pool_address: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[POOL_LP_MINT_SEED, pool_address.as_ref()],
            &PUMP_AMM_PROGRAM_ID,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GlobalConfig
// ─────────────────────────────────────────────────────────────────────────────

/// On-chain GlobalConfig account (singleton at [`crate::constants::GLOBAL_CONFIG`]).
///
/// TODO: Verify exact field ordering against the deployed IDL. The layout below
/// is inferred from on-chain observations and community IDLs; treat it as
/// approximate until confirmed with a live deserialization test.
///
/// Layout (after 8-byte discriminator):
/// ```text
///  admin                     Pubkey
///  lp_fee_basis_points       u64
///  protocol_fee_basis_points u64
///  disable_flags             u8
///  protocol_fee_recipients   [Pubkey; 8]
/// ```
#[derive(Debug, Clone, BorshDeserialize, BorshSerialize)]
pub struct GlobalConfig {
    pub admin: Pubkey,
    pub lp_fee_basis_points: u64,
    pub protocol_fee_basis_points: u64,
    pub disable_flags: u8,
    pub protocol_fee_recipients: [Pubkey; 8],
}

/// Anchor account discriminator for GlobalConfig.
///
/// TODO: Confirm this value against `sha256("account:GlobalConfig")[..8]`.
pub const GLOBAL_CONFIG_DISCRIMINATOR: [u8; 8] = [149, 8, 156, 202, 160, 252, 176, 217];

impl GlobalConfig {
    /// Deserialize from raw account data (skips the 8-byte discriminator).
    pub fn try_from_slice(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Account data too short",
            ));
        }
        let disc = &data[..8];
        if disc != GLOBAL_CONFIG_DISCRIMINATOR {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid GlobalConfig discriminator",
            ));
        }
        BorshDeserialize::try_from_slice(&data[8..])
    }

    /// Total fee in basis points (LP fee + protocol fee).
    pub fn total_fee_bps(&self) -> u64 {
        self.lp_fee_basis_points + self.protocol_fee_basis_points
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PDA helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Derive the `event_authority` PDA used for CPI self-calls in Anchor programs.
///
/// Seeds: `["__event_authority"]`
pub fn derive_event_authority() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[EVENT_AUTHORITY_SEED], &PUMP_AMM_PROGRAM_ID)
}

/// Derive the global config PDA (should equal [`crate::constants::GLOBAL_CONFIG`]).
pub fn derive_global_config() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &PUMP_AMM_PROGRAM_ID)
}
