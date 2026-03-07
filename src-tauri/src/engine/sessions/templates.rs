// Dashboard Templates — CRUD for reusable dashboard blueprints.
// Templates define a set of component skeletons that an agent
// instantiates and populates with live data.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::atoms::types::DashboardTemplateRow;
use rusqlite::params;

impl SessionStore {
    /// Insert a new dashboard template.
    #[allow(clippy::too_many_arguments)]
    pub fn create_template(
        &self,
        id: &str,
        name: &str,
        description: &str,
        icon: &str,
        components_json: &str,
        tags_json: &str,
        setup_prompt: Option<&str>,
        source: &str,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dashboard_templates
                (id, name, description, icon, components, tags, setup_prompt, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                name,
                description,
                icon,
                components_json,
                tags_json,
                setup_prompt,
                source
            ],
        )?;
        Ok(())
    }

    /// Get a single template by ID.
    pub fn get_template(&self, id: &str) -> EngineResult<Option<DashboardTemplateRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, components, tags,
                    setup_prompt, source, created_at
             FROM dashboard_templates WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], map_template_row)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// List all templates, optionally filtering by source.
    pub fn list_templates(&self, source: Option<&str>) -> EngineResult<Vec<DashboardTemplateRow>> {
        let conn = self.conn.lock();
        if let Some(src) = source {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, icon, components, tags,
                        setup_prompt, source, created_at
                 FROM dashboard_templates WHERE source = ?1
                 ORDER BY name ASC",
            )?;
            let rows = stmt
                .query_map(params![src], map_template_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, icon, components, tags,
                        setup_prompt, source, created_at
                 FROM dashboard_templates
                 ORDER BY name ASC",
            )?;
            let rows = stmt
                .query_map([], map_template_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }

    /// Delete a template by ID.
    pub fn delete_template(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute("DELETE FROM dashboard_templates WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Seed built-in templates if they don't already exist.
    /// Called during migration/startup.
    pub fn seed_builtin_templates(&self) -> EngineResult<u64> {
        let builtins = builtin_templates();
        let mut count = 0u64;
        for t in &builtins {
            let conn = self.conn.lock();
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM dashboard_templates WHERE id = ?1",
                params![t.id],
                |row| row.get(0),
            )?;
            if !exists {
                drop(conn); // release lock before calling create_template
                self.create_template(
                    t.id,
                    t.name,
                    t.description,
                    t.icon,
                    t.components,
                    t.tags,
                    t.setup_prompt,
                    "builtin",
                )?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Map a SQLite row to a DashboardTemplateRow.
fn map_template_row(row: &rusqlite::Row) -> rusqlite::Result<DashboardTemplateRow> {
    Ok(DashboardTemplateRow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        icon: row.get(3)?,
        components: row.get(4)?,
        tags: row.get(5)?,
        setup_prompt: row.get(6)?,
        source: row.get(7)?,
        created_at: row.get(8)?,
    })
}

// ── Built-in Template Definitions ────────────────────────────────────────

struct BuiltinTemplate {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    components: &'static str,
    tags: &'static str,
    setup_prompt: Option<&'static str>,
}

fn builtin_templates() -> Vec<BuiltinTemplate> {
    vec![
        BuiltinTemplate {
            id: "trading-overview",
            name: "Trading Overview",
            description: "Portfolio value, P&L chart, open positions table, recent trades log",
            icon: "trending_up",
            components: r#"[
                {"type":"metric","title":"Portfolio Value","data_hint":"Total portfolio USD value"},
                {"type":"metric","title":"P&L Today","data_hint":"Daily P&L with percentage"},
                {"type":"chart","title":"Portfolio (7d)","data_hint":"Daily portfolio values","chart_type":"line"},
                {"type":"table","title":"Open Positions","columns":["Token","Entry","Current","P&L","Size"]},
                {"type":"log","title":"Recent Trades","data_hint":"Timestamped trade entries"}
            ]"#,
            tags: r#"["trading","portfolio","defi"]"#,
            setup_prompt: Some("Fetch the user's portfolio data, open positions, and recent trades to populate this trading dashboard"),
        },
        BuiltinTemplate {
            id: "project-health",
            name: "Project Health",
            description: "Task burndown chart, agent activity log, file change metrics, blockers table",
            icon: "assignment",
            components: r#"[
                {"type":"metric","title":"Open Tasks","data_hint":"Count of incomplete tasks"},
                {"type":"metric","title":"Completed Today","data_hint":"Tasks finished today"},
                {"type":"chart","title":"Task Burndown","data_hint":"Tasks remaining over time","chart_type":"area"},
                {"type":"table","title":"Blockers","columns":["Task","Blocker","Priority","Assigned"]},
                {"type":"log","title":"Recent Activity","data_hint":"Agent and user activity log"}
            ]"#,
            tags: r#"["project","tasks","productivity"]"#,
            setup_prompt: Some("Gather task statistics, recent activity, and any blockers for the active project"),
        },
        BuiltinTemplate {
            id: "system-monitor",
            name: "System Monitor",
            description: "CPU/memory metrics, token usage chart, cost tracker, active channels status",
            icon: "monitor_heart",
            components: r#"[
                {"type":"metric","title":"Token Usage","data_hint":"Tokens used today"},
                {"type":"metric","title":"API Cost","data_hint":"Cost in USD today"},
                {"type":"chart","title":"Token Usage (7d)","data_hint":"Daily token counts","chart_type":"bar"},
                {"type":"status","title":"Active Channels","data_hint":"Connected channel names and status"},
                {"type":"kv","title":"System Info","data_hint":"Key system configuration values"}
            ]"#,
            tags: r#"["system","monitoring","usage"]"#,
            setup_prompt: Some("Gather system metrics including token usage, API costs, and active channel status"),
        },
        BuiltinTemplate {
            id: "research-board",
            name: "Research Board",
            description: "Findings table, source log, key insights cards, topic progress bars",
            icon: "science",
            components: r#"[
                {"type":"metric","title":"Sources Reviewed","data_hint":"Number of sources analyzed"},
                {"type":"table","title":"Key Findings","columns":["Finding","Source","Confidence","Tags"]},
                {"type":"log","title":"Source Log","data_hint":"Timestamped source URLs and summaries"},
                {"type":"card","title":"Key Insights","data_hint":"Markdown summary of top findings"},
                {"type":"progress","title":"Research Progress","data_hint":"Topic completion percentages"}
            ]"#,
            tags: r#"["research","analysis","web"]"#,
            setup_prompt: Some("Compile research findings, sources, and insights for the current research topic"),
        },
        BuiltinTemplate {
            id: "email-digest",
            name: "Email Digest",
            description: "Unread count metric, priority inbox table, thread activity chart",
            icon: "mail",
            components: r#"[
                {"type":"metric","title":"Unread","data_hint":"Count of unread emails"},
                {"type":"metric","title":"Flagged","data_hint":"Count of flagged/starred emails"},
                {"type":"table","title":"Priority Inbox","columns":["From","Subject","Time","Priority"]},
                {"type":"chart","title":"Thread Activity","data_hint":"Email volume over past week","chart_type":"bar"}
            ]"#,
            tags: r#"["email","inbox","communication"]"#,
            setup_prompt: Some("Fetch email inbox data including unread count, priority messages, and thread activity"),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema_for_testing;
    use parking_lot::Mutex;
    use rusqlite::Connection;
    use std::sync::Arc;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        schema_for_testing(&conn);
        SessionStore {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    #[test]
    fn create_and_get_template() {
        let store = test_store();
        store
            .create_template(
                "tpl-1",
                "CI Monitor",
                "Build health dashboard",
                "rocket_launch",
                r#"[{"type":"metric","title":"Pass Rate"}]"#,
                r#"["ci","devops"]"#,
                Some("Fetch CI data"),
                "user",
            )
            .unwrap();
        let tpl = store.get_template("tpl-1").unwrap().unwrap();
        assert_eq!(tpl.name, "CI Monitor");
        assert_eq!(tpl.source, "user");
    }

    #[test]
    fn list_templates_all_and_by_source() {
        let store = test_store();
        store
            .create_template("t-1", "A", "", "x", "[]", "[]", None, "builtin")
            .unwrap();
        store
            .create_template("t-2", "B", "", "x", "[]", "[]", None, "user")
            .unwrap();
        assert_eq!(store.list_templates(None).unwrap().len(), 2);
        assert_eq!(store.list_templates(Some("builtin")).unwrap().len(), 1);
        assert_eq!(store.list_templates(Some("user")).unwrap().len(), 1);
    }

    #[test]
    fn delete_template() {
        let store = test_store();
        store
            .create_template("t-1", "X", "", "x", "[]", "[]", None, "user")
            .unwrap();
        assert!(store.delete_template("t-1").unwrap());
        assert!(!store.delete_template("t-1").unwrap());
    }

    #[test]
    fn seed_builtin_templates() {
        let store = test_store();
        let count = store.seed_builtin_templates().unwrap();
        assert_eq!(count, 5);
        // Idempotent — second run seeds 0.
        let count2 = store.seed_builtin_templates().unwrap();
        assert_eq!(count2, 0);
        let all = store.list_templates(Some("builtin")).unwrap();
        assert_eq!(all.len(), 5);
    }
}
