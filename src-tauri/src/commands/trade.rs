// commands/trade.rs — Thin wrappers for trading history, policy, and position commands.

use crate::commands::state::EngineState;
use crate::engine::types::*;
use log::info;
use tauri::State;

// ── Trading ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_trading_history(
    state: State<'_, EngineState>,
    limit: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    state.store.list_trades(limit.unwrap_or(100)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_trading_summary(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    state.store.daily_trade_summary().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_trading_policy_get(
    state: State<'_, EngineState>,
) -> Result<TradingPolicy, String> {
    match state.store.get_config("trading_policy") {
        Ok(Some(json)) => {
            serde_json::from_str(&json).map_err(|e| format!("Parse error: {}", e))
        }
        Ok(None) => Ok(TradingPolicy::default()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn engine_trading_policy_set(
    state: State<'_, EngineState>,
    policy: TradingPolicy,
) -> Result<(), String> {
    info!("[engine] Updating trading policy: auto_approve={}, max_trade=${}, max_daily=${}, pairs={:?}, transfers={}",
        policy.auto_approve, policy.max_trade_usd, policy.max_daily_loss_usd,
        policy.allowed_pairs, policy.allow_transfers);
    let json = serde_json::to_string(&policy).map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("trading_policy", &json).map_err(|e| e.to_string())
}

// ── Positions (Stop-Loss / Take-Profit) ───────────────────────────────

#[tauri::command]
pub fn engine_positions_list(
    state: State<'_, EngineState>,
    status: Option<String>,
) -> Result<Vec<Position>, String> {
    state.store.list_positions(status.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_position_close(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    info!("[engine] Manually closing position {}", id);
    state.store.close_position(&id, "closed_manual", None).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_position_update_targets(
    state: State<'_, EngineState>,
    id: String,
    stop_loss_pct: f64,
    take_profit_pct: f64,
) -> Result<(), String> {
    info!("[engine] Updating position {} targets: SL={:.0}%, TP={:.1}x", id, stop_loss_pct * 100.0, take_profit_pct);
    state.store.update_position_targets(&id, stop_loss_pct, take_profit_pct).map_err(|e| e.to_string())
}
