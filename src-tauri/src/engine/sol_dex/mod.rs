// Paw Agent Engine — Solana DEX Trading (Jupiter + PumpPortal)
// Self-custody Solana wallet with on-chain swap execution.
//
// Module layout:
//   constants    — KNOWN_TOKENS, API endpoints, slippage defaults
//   helpers      — resolve_token, lamports_to_amount, amount_to_lamports
//   rpc          — rpc_call, get_sol_balance, get_token_accounts, get_mint_info, resolve_decimals_on_chain
//   wallet       — pubkey_from_secret, execute_sol_wallet_create
//   portfolio    — execute_sol_balance, execute_sol_portfolio, execute_sol_token_info
//   jupiter      — execute_sol_quote, execute_sol_quote_jupiter, execute_sol_swap, execute_sol_swap_jupiter
//   pumpportal   — is_jupiter_route_error, pumpportal_get_tx, pumpportal_swap
//   transaction  — sign_solana_transaction, decode/encode_compact_u16, build_solana_transaction, derive_ata
//   transfer     — execute_sol_transfer
//   price        — get_token_price_usd (DexScreener)

pub(crate) mod constants;
pub(crate) mod helpers;
pub(crate) mod rpc;
pub(crate) mod pumpportal;
pub(crate) mod transaction;

pub mod wallet;
pub mod portfolio;
pub mod jupiter;
pub mod transfer;
pub mod price;

// ── Re-exports (preserve crate::engine::sol_dex::* API) ──────────────────────

pub use wallet::execute_sol_wallet_create;
pub use portfolio::{execute_sol_balance, execute_sol_portfolio, execute_sol_token_info};
pub use jupiter::{execute_sol_quote, execute_sol_swap};
pub use transfer::execute_sol_transfer;
pub use price::get_token_price_usd;
