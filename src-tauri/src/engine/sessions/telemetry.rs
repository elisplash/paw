// Paw Agent Engine — Telemetry Metrics Persistence (Canvas Phase 5)
// Stores daily/weekly aggregated metrics in SQLite for dashboard display.
// Agents can query these metrics to generate usage summaries.

use crate::atoms::error::EngineResult;
use crate::atoms::types::TelemetryMetricRow;
use crate::engine::sessions::SessionStore;

impl SessionStore {
    /// Record a telemetry metric for a given date and session.
    #[allow(clippy::too_many_arguments)]
    pub fn record_metric(
        &self,
        date: &str,
        session_id: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        tool_calls: u32,
        tool_duration_ms: u64,
        llm_duration_ms: u64,
        total_duration_ms: u64,
        rounds: u32,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO telemetry_metrics
                (date, session_id, model, input_tokens, output_tokens, cost_usd,
                 tool_calls, tool_duration_ms, llm_duration_ms, total_duration_ms, rounds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                date,
                session_id,
                model,
                input_tokens,
                output_tokens,
                cost_usd,
                tool_calls,
                tool_duration_ms,
                llm_duration_ms,
                total_duration_ms,
                rounds,
            ],
        )?;
        Ok(())
    }

    /// Get aggregated metrics for a single date.
    pub fn get_daily_metrics(&self, date: &str) -> EngineResult<TelemetryDailySummary> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_usd), 0.0),
                COALESCE(SUM(tool_calls), 0),
                COALESCE(SUM(tool_duration_ms), 0),
                COALESCE(SUM(llm_duration_ms), 0),
                COALESCE(SUM(total_duration_ms), 0),
                COALESCE(SUM(rounds), 0),
                COUNT(*)
             FROM telemetry_metrics WHERE date = ?1",
        )?;

        let row = stmt.query_row(rusqlite::params![date], |row| {
            Ok(TelemetryDailySummary {
                date: date.to_string(),
                input_tokens: row.get::<_, i64>(0)? as u64,
                output_tokens: row.get::<_, i64>(1)? as u64,
                cost_usd: row.get(2)?,
                tool_calls: row.get::<_, i64>(3)? as u32,
                tool_duration_ms: row.get::<_, i64>(4)? as u64,
                llm_duration_ms: row.get::<_, i64>(5)? as u64,
                total_duration_ms: row.get::<_, i64>(6)? as u64,
                rounds: row.get::<_, i64>(7)? as u32,
                turn_count: row.get::<_, i64>(8)? as u32,
            })
        })?;
        Ok(row)
    }

    /// Get daily metrics for a date range (inclusive).
    pub fn get_metrics_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> EngineResult<Vec<TelemetryDailySummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT
                date,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_usd), 0.0),
                COALESCE(SUM(tool_calls), 0),
                COALESCE(SUM(tool_duration_ms), 0),
                COALESCE(SUM(llm_duration_ms), 0),
                COALESCE(SUM(total_duration_ms), 0),
                COALESCE(SUM(rounds), 0),
                COUNT(*)
             FROM telemetry_metrics
             WHERE date BETWEEN ?1 AND ?2
             GROUP BY date
             ORDER BY date ASC",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![start_date, end_date], |row| {
                Ok(TelemetryDailySummary {
                    date: row.get(0)?,
                    input_tokens: row.get::<_, i64>(1)? as u64,
                    output_tokens: row.get::<_, i64>(2)? as u64,
                    cost_usd: row.get(3)?,
                    tool_calls: row.get::<_, i64>(4)? as u32,
                    tool_duration_ms: row.get::<_, i64>(5)? as u64,
                    llm_duration_ms: row.get::<_, i64>(6)? as u64,
                    total_duration_ms: row.get::<_, i64>(7)? as u64,
                    rounds: row.get::<_, i64>(8)? as u32,
                    turn_count: row.get::<_, i64>(9)? as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get per-model breakdown for a date.
    pub fn get_model_breakdown(&self, date: &str) -> EngineResult<Vec<TelemetryModelBreakdown>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT
                model,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_usd), 0.0),
                COUNT(*)
             FROM telemetry_metrics
             WHERE date = ?1
             GROUP BY model
             ORDER BY SUM(cost_usd) DESC",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![date], |row| {
                Ok(TelemetryModelBreakdown {
                    model: row.get(0)?,
                    input_tokens: row.get::<_, i64>(1)? as u64,
                    output_tokens: row.get::<_, i64>(2)? as u64,
                    cost_usd: row.get(3)?,
                    turn_count: row.get::<_, i64>(4)? as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List individual metric rows for a session (for Inspector detail view).
    pub fn list_session_metrics(&self, session_id: &str) -> EngineResult<Vec<TelemetryMetricRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, date, session_id, model, input_tokens, output_tokens,
                    cost_usd, tool_calls, tool_duration_ms, llm_duration_ms,
                    total_duration_ms, rounds, created_at
             FROM telemetry_metrics
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![session_id], |row| {
                Ok(TelemetryMetricRow {
                    id: row.get(0)?,
                    date: row.get(1)?,
                    session_id: row.get(2)?,
                    model: row.get(3)?,
                    input_tokens: row.get::<_, i64>(4)? as u64,
                    output_tokens: row.get::<_, i64>(5)? as u64,
                    cost_usd: row.get(6)?,
                    tool_calls: row.get::<_, i64>(7)? as u32,
                    tool_duration_ms: row.get::<_, i64>(8)? as u64,
                    llm_duration_ms: row.get::<_, i64>(9)? as u64,
                    total_duration_ms: row.get::<_, i64>(10)? as u64,
                    rounds: row.get::<_, i64>(11)? as u32,
                    created_at: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete metrics older than a cutoff date (cleanup).
    pub fn purge_metrics_before(&self, cutoff_date: &str) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM telemetry_metrics WHERE date < ?1",
            rusqlite::params![cutoff_date],
        )?;
        Ok(deleted as u64)
    }
}

// ── Aggregate Types ───────────────────────────────────────────────────

/// Aggregated metrics for a single day.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelemetryDailySummary {
    pub date: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub tool_calls: u32,
    pub tool_duration_ms: u64,
    pub llm_duration_ms: u64,
    pub total_duration_ms: u64,
    pub rounds: u32,
    pub turn_count: u32,
}

/// Per-model breakdown for a single day.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelemetryModelBreakdown {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub turn_count: u32,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> SessionStore {
        SessionStore::open(":memory:").expect("in-memory store")
    }

    #[test]
    fn record_and_query_daily() {
        let store = test_store();
        store
            .record_metric(
                "2026-03-04",
                "sess-1",
                "claude-sonnet-4",
                50000,
                10000,
                0.47,
                5,
                1200,
                3500,
                5000,
                3,
            )
            .unwrap();

        let daily = store.get_daily_metrics("2026-03-04").unwrap();
        assert_eq!(daily.input_tokens, 50000);
        assert_eq!(daily.output_tokens, 10000);
        assert!((daily.cost_usd - 0.47).abs() < 0.001);
        assert_eq!(daily.tool_calls, 5);
        assert_eq!(daily.turn_count, 1);
    }

    #[test]
    fn metrics_range() {
        let store = test_store();
        store
            .record_metric(
                "2026-03-01",
                "s1",
                "gpt-4o",
                1000,
                500,
                0.01,
                1,
                100,
                200,
                400,
                1,
            )
            .unwrap();
        store
            .record_metric(
                "2026-03-02",
                "s2",
                "gpt-4o",
                2000,
                1000,
                0.02,
                2,
                200,
                300,
                600,
                2,
            )
            .unwrap();
        store
            .record_metric(
                "2026-03-03",
                "s3",
                "claude-sonnet-4",
                3000,
                1500,
                0.05,
                3,
                300,
                400,
                800,
                3,
            )
            .unwrap();

        let range = store.get_metrics_range("2026-03-01", "2026-03-03").unwrap();
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].date, "2026-03-01");
        assert_eq!(range[2].date, "2026-03-03");
    }

    #[test]
    fn model_breakdown() {
        let store = test_store();
        store
            .record_metric(
                "2026-03-04",
                "s1",
                "claude-sonnet-4",
                5000,
                1000,
                0.10,
                2,
                100,
                200,
                400,
                1,
            )
            .unwrap();
        store
            .record_metric(
                "2026-03-04",
                "s2",
                "gpt-4o",
                3000,
                800,
                0.05,
                1,
                50,
                150,
                300,
                1,
            )
            .unwrap();
        store
            .record_metric(
                "2026-03-04",
                "s3",
                "claude-sonnet-4",
                4000,
                900,
                0.08,
                3,
                200,
                350,
                600,
                2,
            )
            .unwrap();

        let breakdown = store.get_model_breakdown("2026-03-04").unwrap();
        assert_eq!(breakdown.len(), 2);
        // Sorted by cost DESC, so claude should be first
        assert_eq!(breakdown[0].model, "claude-sonnet-4");
        assert_eq!(breakdown[0].turn_count, 2);
    }

    #[test]
    fn purge_old_metrics() {
        let store = test_store();
        store
            .record_metric(
                "2026-01-01",
                "s1",
                "gpt-4o",
                1000,
                500,
                0.01,
                1,
                100,
                200,
                400,
                1,
            )
            .unwrap();
        store
            .record_metric(
                "2026-03-04",
                "s2",
                "gpt-4o",
                2000,
                1000,
                0.02,
                2,
                200,
                300,
                600,
                2,
            )
            .unwrap();

        let purged = store.purge_metrics_before("2026-03-01").unwrap();
        assert_eq!(purged, 1);

        let range = store.get_metrics_range("2026-01-01", "2026-12-31").unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].date, "2026-03-04");
    }
}
