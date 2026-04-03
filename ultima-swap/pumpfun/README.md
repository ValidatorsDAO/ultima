# ultima-swap-pumpfun

Rust SDK for the **PumpSwap AMM** (Pump.fun graduated-token pools) on Solana.

## Scope

PumpSwap is Pump.fun's native constant-product AMM for tokens that have
graduated from the bonding curve.  This crate lets you:

- **Quote** exact buy/sell amounts using constant-product math with fee-on-input.
- **Build instructions** for `buy` and `sell` against any PumpSwap pool.
- **Detect `create_pool`** transactions in real-time (monitoring / sniper bots).
- **Deserialize** on-chain `Pool` and `GlobalConfig` accounts from raw bytes.

## Current Status

| Area | Status |
|------|--------|
| Program IDs & discriminators | ✅ Confirmed against mainnet |
| `Pool` account layout | ✅ Confirmed |
| `GlobalConfig` account layout | ⚠️ Inferred — verify discriminator + field ordering against live IDL |
| `buy` / `sell` instruction builders | ✅ Account ordering matches IDL |
| Constant-product AMM math | ✅ Tested |
| Token-2022 base mints | ⚠️ Account list passes `token_2022_program` but ATA creation uses `token_program`; caller must pass the correct program for Token-2022 mints |
| `create_pool` building | ❌ Not implemented — handled server-side by Pump.fun migration service |
| LP deposit / withdraw | ❌ Not yet implemented |

## Quick Start

```toml
[dependencies]
ultima-swap-pumpfun = { path = "../ultima-swap/pumpfun" }
```

```rust
use ultima_swap_pumpfun::{
    accounts::Pool,
    instructions::{build_buy, BuyParams},
    math::{quote_in_for_exact_base_out, with_slippage_max, DEFAULT_FEE_BPS},
};

// 1. Fetch pool account bytes from RPC and deserialize.
let pool_data = Pool::try_from_slice(&account_data)?;

// 2. Fetch token vault balances (base and quote reserves).
let quote_needed = quote_in_for_exact_base_out(
    base_reserves,
    quote_reserves,
    1_000_000,       // base atoms out
    DEFAULT_FEE_BPS, // 30 bps (25 LP + 5 protocol)
)?;

// 3. Add 1 % slippage buffer.
let max_quote = with_slippage_max(quote_needed, 100)?;

// 4. Build the instruction.
let ix = build_buy(BuyParams {
    pool: pool_address,
    pool_data,
    user,
    base_amount_out: 1_000_000,
    max_quote_amount_in: max_quote,
    fee_recipient_index: 0, // rotate 0–7 across transactions for throughput
})?;
```

## Architecture

```
ultima-swap-pumpfun/
├── src/
│   ├── lib.rs          — public API & re-exports
│   ├── constants.rs    — program IDs, discriminators, PDA seeds, fee recipients
│   ├── accounts.rs     — Pool, GlobalConfig, PDA helpers
│   ├── instructions.rs — build_buy, build_sell, try_parse_create_pool
│   ├── math.rs         — AMM quote math, slippage helpers, price impact
│   └── error.rs        — SwapError, SwapResult<T>
└── README.md
```

## Fee Model

Fees are charged on the **input** token:

```
effective_input = gross_input * (10_000 − total_fee_bps) / 10_000
output          = reserve_out * effective_input / (reserve_in + effective_input)
```

Default mainnet fee: **30 bps** (25 LP + 5 protocol).  Live values are stored
in the singleton `GlobalConfig` account at
`ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw`.

## Known TODOs

- [ ] Confirm `GlobalConfig` Borsh discriminator and field ordering against
      the deployed IDL (`src/accounts.rs` line ~100).
- [ ] Support Token-2022 base mints: `create_base_ata_if_needed` needs to pass
      `token_2022_program` when the mint is a Token-2022 mint.
- [ ] Fee-recipient rotation strategy (currently caller-controlled via index).
- [ ] LP deposit / withdraw instruction builders.
- [ ] Integration tests that hit a localnet/devnet fork.
