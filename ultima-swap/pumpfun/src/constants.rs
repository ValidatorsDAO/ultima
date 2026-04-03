use solana_pubkey::Pubkey;

/// PumpSwap AMM program ID
pub const PUMP_AMM_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");

/// GlobalConfig PDA address (seeds: ["global_config"])
pub const GLOBAL_CONFIG: Pubkey =
    solana_pubkey::pubkey!("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw");

/// SPL Token program
pub const TOKEN_PROGRAM: Pubkey =
    solana_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// SPL Token-2022 program
pub const TOKEN_2022_PROGRAM: Pubkey =
    solana_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Associated Token Account program
pub const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    solana_pubkey::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

/// System program
pub const SYSTEM_PROGRAM: Pubkey =
    solana_pubkey::pubkey!("11111111111111111111111111111111");

/// Wrapped SOL mint
pub const WSOL_MINT: Pubkey =
    solana_pubkey::pubkey!("So11111111111111111111111111111111111111112");

/// Protocol fee recipients (from GlobalConfig on mainnet).
/// Randomly pick one per tx for throughput.
pub const PROTOCOL_FEE_RECIPIENTS: [Pubkey; 8] = [
    solana_pubkey::pubkey!("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV"),
    solana_pubkey::pubkey!("7VtfL8fvgNfhz17qKRMjzQEXgbdpnHHHQRh54R9jP2RJ"),
    solana_pubkey::pubkey!("7hTckgnGnLQR6sdH7YkqFTAA7VwTfYFaZ6EhEsU3saCX"),
    solana_pubkey::pubkey!("9rPYyANsfQZw3DnDmKE3YCQF5E8oD89UXoHn9JFEhJUz"),
    solana_pubkey::pubkey!("AVmoTthdrX6tKt4nDjco2D775W2YK3sDhxPcMmzUAmTY"),
    solana_pubkey::pubkey!("FWsW1xNtWscwNmKv6wVsU1iTzRN6wmmk3MjxRP5tT7hz"),
    solana_pubkey::pubkey!("G5UZAVbAf46s7cKWoyKu8kYTip9DGTpbLZ2qa9Aq69dP"),
    solana_pubkey::pubkey!("JCRGumoE9Qi5BBgULTgdgTLjSgkCMSbF62ZZfGs84JeU"),
];

// ── Anchor instruction discriminators (first 8 bytes of SHA256("global:<name>")) ──

/// buy discriminator
pub const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];

/// sell discriminator
pub const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

/// create_pool discriminator (for detection)
pub const CREATE_POOL_DISCRIMINATOR: [u8; 8] = [233, 146, 209, 142, 207, 104, 64, 188];

// ── PDA seeds ──

pub const POOL_SEED: &[u8] = b"pool";
pub const POOL_LP_MINT_SEED: &[u8] = b"pool_lp_mint";
pub const GLOBAL_CONFIG_SEED: &[u8] = b"global_config";
pub const EVENT_AUTHORITY_SEED: &[u8] = b"__event_authority";
