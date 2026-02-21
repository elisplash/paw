// Paw Agent Engine — Trading Auto-Approval Policy
//
// Policy-based auto-approval for trading write tools (Coinbase, Solana, EVM DEX).
// Checks configurable limits (max trade size, daily loss, allowed pairs, transfer caps).

use crate::engine::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::Manager;

/// Policy-based auto-approval for trading write tools (all chains: Coinbase, Solana, EVM DEX).
/// Checks configurable limits (max trade size, daily loss, allowed pairs, transfer caps).
/// Returns false (requiring HIL) when no policy is configured or limits exceeded.
pub(crate) fn check_trading_auto_approve(tool_name: &str, args_str: &str, app_handle: &tauri::AppHandle) -> bool {
    // Trading write tools — wallet creation, swaps, transfers
    match tool_name {
        "coinbase_trade" | "coinbase_transfer" | "coinbase_wallet_create"
        | "sol_swap" | "sol_transfer" | "sol_wallet_create"
        | "dex_swap" | "dex_transfer" | "dex_wallet_create" => {}
        _ => return false,
    }

    // Load trading policy from engine config
    let state = match app_handle.try_state::<EngineState>() {
        Some(s) => s,
        None => return false,
    };

    let policy: TradingPolicy = match state.store.get_config("trading_policy") {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => TradingPolicy::default(),
    };

    if !policy.auto_approve {
        return false;
    }

    // Wallet creation is always safe if auto-approve is on
    if tool_name.ends_with("_wallet_create") {
        info!("[engine] Auto-approved {} via trading policy", tool_name);
        return true;
    }

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();

    // Swap/Trade tools — check trade size and daily limits
    if tool_name == "coinbase_trade" || tool_name == "sol_swap" || tool_name == "dex_swap" {
        let amount: f64 = args["amount"].as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| args["amount"].as_f64())
            .unwrap_or(f64::MAX);
        let product_id = args["product_id"].as_str()
            .or_else(|| args["input_mint"].as_str())
            .or_else(|| args["token_in"].as_str())
            .unwrap_or("");

        // Check trade size
        if amount > policy.max_trade_usd {
            info!("[engine] Trade ${:.2} exceeds max ${:.2} — requiring HIL", amount, policy.max_trade_usd);
            return false;
        }

        // Check allowed pairs
        if !policy.allowed_pairs.is_empty() {
            let pair_upper = product_id.to_uppercase();
            if !policy.allowed_pairs.iter().any(|p| p.to_uppercase() == pair_upper) {
                info!("[engine] Pair {} not in allowed list — requiring HIL", product_id);
                return false;
            }
        }

        // Check daily spending limit
        if let Ok(summary) = state.store.daily_trade_summary() {
            let daily_spent = summary["daily_spent_usd"].as_f64().unwrap_or(0.0);
            if daily_spent + amount > policy.max_daily_loss_usd {
                info!("[engine] Daily spend ${:.2} + ${:.2} exceeds max ${:.2} — requiring HIL",
                    daily_spent, amount, policy.max_daily_loss_usd);
                return false;
            }
        }

        info!("[engine] Auto-approved {} ${:.2} {} via trading policy", tool_name, amount, product_id);
        return true;
    }

    // Transfer tools — check transfer limits
    if tool_name == "coinbase_transfer" || tool_name == "sol_transfer" || tool_name == "dex_transfer" {
        if !policy.allow_transfers {
            info!("[engine] Transfers not auto-approved in trading policy");
            return false;
        }

        let amount: f64 = args["amount"].as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| args["amount"].as_f64())
            .unwrap_or(f64::MAX);

        if amount > policy.max_transfer_usd {
            info!("[engine] Transfer ${:.2} exceeds max ${:.2} — requiring HIL", amount, policy.max_transfer_usd);
            return false;
        }

        // Check daily spending limit (transfers count toward it too)
        if let Ok(summary) = state.store.daily_trade_summary() {
            let daily_spent = summary["daily_spent_usd"].as_f64().unwrap_or(0.0);
            if daily_spent + amount > policy.max_daily_loss_usd {
                info!("[engine] Daily spend ${:.2} + ${:.2} exceeds max ${:.2} — requiring HIL",
                    daily_spent, amount, policy.max_daily_loss_usd);
                return false;
            }
        }

        info!("[engine] Auto-approved {} ${:.2} via trading policy", tool_name, amount);
        return true;
    }

    false
}
