use log::info;
use rusqlite::params;
use crate::engine::types::Position;
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl SessionStore {
    // ── Positions (Stop-Loss / Take-Profit) ────────────────────────────

    /// Insert a new open position.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_position(
        &self,
        mint: &str,
        symbol: &str,
        entry_price_usd: f64,
        entry_sol: f64,
        amount: f64,
        stop_loss_pct: f64,
        take_profit_pct: f64,
        agent_id: Option<&str>,
    ) -> EngineResult<String> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO positions (id, mint, symbol, entry_price_usd, entry_sol, amount, current_amount, stop_loss_pct, take_profit_pct, status, last_price_usd, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, 'open', ?4, ?9)",
            params![id, mint, symbol, entry_price_usd, entry_sol, amount, stop_loss_pct, take_profit_pct, agent_id],
        )?;
        info!("[positions] Opened position {} for {} ({}) — entry ${:.6}, SL {:.0}%, TP {:.0}x",
            id, symbol, &mint[..std::cmp::min(8, mint.len())], entry_price_usd, stop_loss_pct * 100.0, take_profit_pct);
        Ok(id)
    }

    /// List all positions, optionally filtered by status.
    pub fn list_positions(&self, status_filter: Option<&str>) -> EngineResult<Vec<Position>> {
        let conn = self.conn.lock();
        // Use (?1 IS NULL OR status = ?1) so one parameterized query handles both cases.
        let sql = "SELECT id, mint, symbol, entry_price_usd, entry_sol, amount, current_amount,
                          stop_loss_pct, take_profit_pct, status, last_price_usd, last_checked_at,
                          created_at, closed_at, close_tx, agent_id
                   FROM positions WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![status_filter], |row| {
            Ok(Position {
                id: row.get(0)?,
                mint: row.get(1)?,
                symbol: row.get(2)?,
                entry_price_usd: row.get(3)?,
                entry_sol: row.get(4)?,
                amount: row.get(5)?,
                current_amount: row.get(6)?,
                stop_loss_pct: row.get(7)?,
                take_profit_pct: row.get(8)?,
                status: row.get(9)?,
                last_price_usd: row.get(10)?,
                last_checked_at: row.get(11)?,
                created_at: row.get(12)?,
                closed_at: row.get(13)?,
                close_tx: row.get(14)?,
                agent_id: row.get(15)?,
            })
        })?;
        let mut positions = Vec::new();
        for row in rows {
            positions.push(row?);
        }
        Ok(positions)
    }

    /// Update a position's last known price.
    pub fn update_position_price(&self, id: &str, price_usd: f64) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET last_price_usd = ?1, last_checked_at = datetime('now') WHERE id = ?2",
            params![price_usd, id],
        )?;
        Ok(())
    }

    /// Close a position (stop-loss hit, take-profit hit, or manual).
    pub fn close_position(&self, id: &str, status: &str, close_tx: Option<&str>) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET status = ?1, closed_at = datetime('now'), close_tx = ?2 WHERE id = ?3",
            params![status, close_tx, id],
        )?;
        Ok(())
    }

    /// Reduce the current_amount of a position (partial take-profit sell).
    pub fn reduce_position(&self, id: &str, new_amount: f64) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET current_amount = ?1 WHERE id = ?2",
            params![new_amount, id],
        )?;
        Ok(())
    }

    /// Update stop-loss and take-profit percentages for a position.
    pub fn update_position_targets(&self, id: &str, stop_loss_pct: f64, take_profit_pct: f64) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET stop_loss_pct = ?1, take_profit_pct = ?2 WHERE id = ?3",
            params![stop_loss_pct, take_profit_pct, id],
        )?;
        Ok(())
    }
}
