use chrono::Utc;
use rusqlite::params;
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl SessionStore {
    // ── Trade History ──────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn insert_trade(
        &self,
        trade_type: &str,
        side: Option<&str>,
        product_id: Option<&str>,
        currency: Option<&str>,
        amount: &str,
        order_type: Option<&str>,
        order_id: Option<&str>,
        status: &str,
        usd_value: Option<&str>,
        to_address: Option<&str>,
        reason: &str,
        session_id: Option<&str>,
        agent_id: Option<&str>,
        raw_response: Option<&str>,
    ) -> EngineResult<String> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO trade_history (id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, raw_response)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, raw_response],
        )?;
        Ok(id)
    }

    pub fn list_trades(&self, limit: u32) -> EngineResult<Vec<serde_json::Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, created_at
             FROM trade_history ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "trade_type": row.get::<_, String>(1)?,
                "side": row.get::<_, Option<String>>(2)?,
                "product_id": row.get::<_, Option<String>>(3)?,
                "currency": row.get::<_, Option<String>>(4)?,
                "amount": row.get::<_, String>(5)?,
                "order_type": row.get::<_, Option<String>>(6)?,
                "order_id": row.get::<_, Option<String>>(7)?,
                "status": row.get::<_, String>(8)?,
                "usd_value": row.get::<_, Option<String>>(9)?,
                "to_address": row.get::<_, Option<String>>(10)?,
                "reason": row.get::<_, String>(11)?,
                "session_id": row.get::<_, Option<String>>(12)?,
                "agent_id": row.get::<_, Option<String>>(13)?,
                "created_at": row.get::<_, String>(14)?,
            }))
        })?;
        let mut trades = Vec::new();
        for row in rows {
            trades.push(row?);
        }
        Ok(trades)
    }

    /// Get daily P&L: sum of all trades today, grouped by side
    pub fn daily_trade_summary(&self) -> EngineResult<serde_json::Value> {
        let conn = self.conn.lock();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        // SQLite datetime('now') uses space separator: "2026-02-19 00:00:00"
        // Must match that format, NOT ISO 8601 'T' separator
        let today_start = format!("{} 00:00:00", today);

        // Coinbase trades
        let trade_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'trade' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        let transfer_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'transfer' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        // DEX swap count
        let dex_swap_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'dex_swap' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        // Sum USD values for buys and sells today (Coinbase)
        let buy_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'trade' AND side = 'buy' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        let sell_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'trade' AND side = 'sell' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        let transfer_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'transfer' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // DEX swap volume (sum of amounts — not USD-denominated, but tracks activity)
        let dex_volume_raw: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(amount AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'dex_swap' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Unique tokens swapped today
        let dex_pairs: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT product_id FROM trade_history WHERE trade_type = 'dex_swap' AND product_id IS NOT NULL AND created_at >= ?1"
            ).unwrap();
            let rows = stmt.query_map(params![&today_start], |row| row.get::<_, String>(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };

        // Total operations today (buys + transfers out) for daily loss tracking
        let daily_spent = buy_total + transfer_total;

        Ok(serde_json::json!({
            "date": today,
            "trade_count": trade_count,
            "transfer_count": transfer_count,
            "dex_swap_count": dex_swap_count,
            "buy_total_usd": buy_total,
            "sell_total_usd": sell_total,
            "transfer_total_usd": transfer_total,
            "dex_volume_raw": dex_volume_raw,
            "dex_pairs": dex_pairs,
            "net_pnl_usd": sell_total - buy_total,
            "daily_spent_usd": daily_spent,
        }))
    }
}
