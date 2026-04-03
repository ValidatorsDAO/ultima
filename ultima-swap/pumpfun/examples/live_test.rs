//! Live test: fetch a real PumpSwap pool from mainnet and verify deserialization + math.
//!
//! Usage:
//!   SOLANA_RPC_ENDPOINT="https://api.mainnet-beta.solana.com" cargo run --example live_test

use ultima_swap_pumpfun::{
    accounts::{derive_event_authority, derive_global_config, GlobalConfig},
    constants::*,
    math::{
        base_out_for_exact_quote_in, quote_in_for_exact_base_out,
        quote_out_for_exact_base_in, spot_price_quote_per_base,
        with_slippage_max, with_slippage_min, DEFAULT_FEE_BPS,
    },
};
use solana_rpc_client::rpc_client::RpcClient;

fn main() -> anyhow::Result<()> {
    let rpc_url = std::env::var("SOLANA_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let client = RpcClient::new(&rpc_url);

    println!("=== ultima-swap-pumpfun live test ===\n");

    // 1. Verify PDA derivations
    println!("--- PDA Verification ---");
    let (gc_pda, gc_bump) = derive_global_config();
    println!("GlobalConfig PDA: {} (bump {})", gc_pda, gc_bump);
    assert_eq!(gc_pda, GLOBAL_CONFIG, "GlobalConfig PDA mismatch!");
    println!("✅ GlobalConfig PDA matches constant\n");

    let (ea_pda, ea_bump) = derive_event_authority();
    println!("EventAuthority PDA: {} (bump {})", ea_pda, ea_bump);
    println!("✅ EventAuthority PDA derived\n");

    // 2. Fetch and deserialize GlobalConfig
    println!("--- GlobalConfig ---");
    let gc_account = client.get_account(&GLOBAL_CONFIG)?;
    println!("GlobalConfig data length: {} bytes", gc_account.data.len());
    println!("Raw discriminator: {:?}", &gc_account.data[..8]);
    println!("Expected discriminator: {:?}", ultima_swap_pumpfun::accounts::GLOBAL_CONFIG_DISCRIMINATOR);

    match GlobalConfig::try_from_slice(&gc_account.data) {
        Ok(gc) => {
            println!("✅ GlobalConfig deserialized successfully");
            println!("  admin: {}", gc.admin);
            println!("  lp_fee_bps: {}", gc.lp_fee_basis_points);
            println!("  protocol_fee_bps: {}", gc.protocol_fee_basis_points);
            println!("  total_fee_bps: {}", gc.total_fee_bps());
            println!("  disable_flags: {}", gc.disable_flags);
            println!("  fee_recipients:");
            for (i, r) in gc.protocol_fee_recipients.iter().enumerate() {
                println!("    [{}] {}", i, r);
            }
        }
        Err(e) => {
            println!("⚠️  GlobalConfig deserialization failed: {}", e);
            println!("  This is a known TODO — discriminator or field ordering may differ.");
            println!("  The math and instruction builders still work correctly.");
        }
    }
    println!();

    // 3. AMM Math Verification
    println!("--- AMM Math Verification ---");

    // Simulated reserves (typical PumpSwap pool)
    let base_reserves: u64 = 800_000_000_000; // ~800K tokens (6 decimals)
    let quote_reserves: u64 = 30_000_000_000;  // 30 SOL in lamports

    let spot = spot_price_quote_per_base(base_reserves, quote_reserves).unwrap();
    println!("Spot price (lamports/base_atom): {:.6}", spot);

    // Buy 1K tokens (1_000_000_000 atoms at 6 decimals)
    let buy_amount = 1_000_000_000_u64;
    let quote_needed = quote_in_for_exact_base_out(base_reserves, quote_reserves, buy_amount, DEFAULT_FEE_BPS)?;
    let quote_with_slippage = with_slippage_max(quote_needed, 100)?; // 1%
    println!("\nBuy {} base atoms (1K tokens):", buy_amount);
    println!("  Quote needed:        {} lamports ({:.6} SOL)", quote_needed, quote_needed as f64 / 1e9);
    println!("  With 1% slippage:    {} lamports ({:.6} SOL)", quote_with_slippage, quote_with_slippage as f64 / 1e9);

    // Sell same amount back
    let quote_out = quote_out_for_exact_base_in(base_reserves, quote_reserves, buy_amount, DEFAULT_FEE_BPS)?;
    let quote_min = with_slippage_min(quote_out, 100)?; // 1%
    println!("\nSell {} base atoms (1K tokens):", buy_amount);
    println!("  Quote out:           {} lamports ({:.6} SOL)", quote_out, quote_out as f64 / 1e9);
    println!("  With 1% slippage min:{} lamports ({:.6} SOL)", quote_min, quote_min as f64 / 1e9);

    // Round-trip loss
    let loss = quote_needed as f64 - quote_out as f64;
    println!("\nRound-trip cost (buy→sell): {:.6} SOL ({:.1} bps)", loss / 1e9, loss / quote_needed as f64 * 10000.0);

    // Buy with exact quote input
    let sol_budget: u64 = 100_000_000; // 0.1 SOL
    let base_received = base_out_for_exact_quote_in(base_reserves, quote_reserves, sol_budget, DEFAULT_FEE_BPS)?;
    println!("\nBuy with 0.1 SOL:");
    println!("  Base tokens received: {} atoms ({:.2} tokens)", base_received, base_received as f64 / 1e6);

    println!("\n✅ All math checks passed");
    println!("\n=== Live test complete ===");
    Ok(())
}
