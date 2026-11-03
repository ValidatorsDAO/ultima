use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;

use crate::constants::*;

// ─────────────────────────────────────────────────────────────────────────────
// Pool
// ─────────────────────────────────────────────────────────────────────────────

/// On-chain Pool account.
///
/// Layout (after 8-byte discriminator), checked on mainnet 2026-11-03:
/// ```text
///  pool_bump               u8
///  index                   u16
///  creator                 Pubkey
///  base_mint               Pubkey  (the token, for pump.fun graduations)
///  quote_mint              Pubkey  (WSOL, for pump.fun graduations)
///  lp_mint                 Pubkey
///  pool_base_token_account Pubkey
///  pool_quote_token_account Pubkey
///  lp_supply               u64
///  coin_creator            Pubkey
///  is_mayhem_mode          bool
///  ...                     (more fields; accounts are 301 bytes)
/// ```
///
/// Pools made by calling `create_pool` directly are often the other way
/// round: WSOL as the base and the token as the quote.
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
    pub coin_creator: Pubkey,
    pub is_mayhem_mode: bool,
}

/// Anchor account discriminator for Pool (sha256("account:Pool")[..8]).
pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

impl Pool {
    /// Deserialize from raw account data (skips the 8-byte discriminator).
    ///
    /// Trailing bytes after `is_mayhem_mode` (fields this struct does not
    /// carry yet) are ignored.
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
        let mut cursor = std::io::Cursor::new(&data[8..]);
        BorshDeserialize::deserialize_reader(&mut cursor)
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
    ///
    /// Uses a reader so that extra trailing bytes (from newer program versions
    /// that added fields) are silently ignored.
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
        // Use a reader to allow trailing bytes from newer program versions.
        let mut cursor = std::io::Cursor::new(&data[8..]);
        BorshDeserialize::deserialize_reader(&mut cursor)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A 301-byte Pool account as on mainnet: a coin creator, then
    /// `is_mayhem_mode`, then bytes this struct does not read.
    fn raw_pool(pool: &Pool) -> Vec<u8> {
        let mut raw = POOL_DISCRIMINATOR.to_vec();
        raw.push(pool.pool_bump);
        raw.extend_from_slice(&pool.index.to_le_bytes());
        for key in [
            pool.creator,
            pool.base_mint,
            pool.quote_mint,
            pool.lp_mint,
            pool.pool_base_token_account,
            pool.pool_quote_token_account,
        ] {
            raw.extend_from_slice(key.as_ref());
        }
        raw.extend_from_slice(&pool.lp_supply.to_le_bytes());
        raw.extend_from_slice(pool.coin_creator.as_ref());
        raw.push(pool.is_mayhem_mode as u8);
        raw.resize(301, 0);
        raw
    }

    fn graduated_pool(is_mayhem_mode: bool) -> Pool {
        Pool {
            pool_bump: 255,
            index: 0,
            creator: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: WSOL_MINT,
            lp_mint: Pubkey::new_unique(),
            pool_base_token_account: Pubkey::new_unique(),
            pool_quote_token_account: Pubkey::new_unique(),
            lp_supply: 4_208_723_649_508,
            // Its first byte is not 0 or 1, so read as a bool it fails.
            coin_creator: Pubkey::new_from_array([7; 32]),
            is_mayhem_mode,
        }
    }

    #[test]
    fn a_pool_with_a_coin_creator_reads_it_before_mayhem() {
        for mayhem in [false, true] {
            let expected = graduated_pool(mayhem);
            let pool = Pool::try_from_slice(&raw_pool(&expected)).unwrap();
            assert_eq!(pool.coin_creator, expected.coin_creator);
            assert_eq!(pool.is_mayhem_mode, mayhem);
            assert_eq!(pool.lp_supply, expected.lp_supply);
            assert_eq!(pool.base_mint, expected.base_mint);
            assert_eq!(pool.quote_mint, WSOL_MINT);
        }
    }

    #[test]
    fn a_pool_account_must_reach_is_mayhem_mode() {
        let raw = raw_pool(&graduated_pool(false));
        let end = 8 + 1 + 2 + 6 * 32 + 8 + 32 + 1;
        assert!(Pool::try_from_slice(&raw[..end]).is_ok());
        assert!(Pool::try_from_slice(&raw[..end - 1]).is_err());
        let mut wrong = raw.clone();
        wrong[0] ^= 1;
        assert!(Pool::try_from_slice(&wrong).is_err());
    }
}
