// Paw Agent Engine — Channel Routing
// Resolves which agent should handle messages from a given channel.
// Config is stored in the engine's config store (engine_config table).

use crate::engine::sessions::SessionStore;
use crate::atoms::error::EngineResult;
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    /// Channel type: "telegram", "discord", "irc", "slack", "matrix", "*"
    pub channel: String,
    /// Optional user ID filter. Empty = all users.
    #[serde(default)]
    pub user_filter: Vec<String>,
    /// Optional channel/group ID filter. Empty = all.
    #[serde(default)]
    pub channel_id_filter: Vec<String>,
    /// Agent ID to route to.
    pub agent_id: String,
    /// Human-readable label.
    pub label: String,
    /// Whether this rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Ordered list of routing rules (first match wins).
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
    /// Default agent ID for messages that don't match any rule.
    #[serde(default = "default_agent")]
    pub default_agent_id: String,
}

fn default_agent() -> String { "default".into() }

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            rules: vec![],
            default_agent_id: "default".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub agent_id: String,
    pub matched_rule_id: Option<String>,
    pub matched_rule_label: Option<String>,
}

// ── Config Persistence ─────────────────────────────────────────────────────

const CONFIG_KEY: &str = "channel_routing";

pub fn load_routing_config(store: &Arc<SessionStore>) -> RoutingConfig {
    match store.get_config(CONFIG_KEY) {
        Ok(Some(json)) => {
            serde_json::from_str(&json).unwrap_or_default()
        }
        _ => RoutingConfig::default(),
    }
}

pub fn save_routing_config(store: &Arc<SessionStore>, config: &RoutingConfig) -> EngineResult<()> {
    let json = serde_json::to_string(config)?;
    store.set_config(CONFIG_KEY, &json)
}

// ── Route Resolution ───────────────────────────────────────────────────────

/// Resolve which agent should handle a message.
/// Evaluates rules in order — first match wins.
pub fn resolve_route(
    config: &RoutingConfig,
    channel: &str,
    user_id: &str,
    channel_id: Option<&str>,
) -> RouteResult {
    for rule in &config.rules {
        if !rule.enabled {
            continue;
        }

        // Channel match: "*" matches all, or exact match
        if rule.channel != "*" && rule.channel != channel {
            continue;
        }

        // User filter: empty = matches all users
        if !rule.user_filter.is_empty() && !rule.user_filter.contains(&user_id.to_string()) {
            continue;
        }

        // Channel ID filter: empty = matches all
        if !rule.channel_id_filter.is_empty() {
            if let Some(cid) = channel_id {
                if !rule.channel_id_filter.contains(&cid.to_string()) {
                    continue;
                }
            }
        }

        info!(
            "[routing] Matched rule '{}': {} → agent '{}'",
            rule.label, channel, rule.agent_id
        );

        return RouteResult {
            agent_id: rule.agent_id.clone(),
            matched_rule_id: Some(rule.id.clone()),
            matched_rule_label: Some(rule.label.clone()),
        };
    }

    // No rule matched — use default
    RouteResult {
        agent_id: config.default_agent_id.clone(),
        matched_rule_id: None,
        matched_rule_label: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(channel: &str, agent: &str, label: &str) -> RoutingRule {
        RoutingRule {
            id: format!("test_{}", label),
            channel: channel.into(),
            user_filter: vec![],
            channel_id_filter: vec![],
            agent_id: agent.into(),
            label: label.into(),
            enabled: true,
        }
    }

    #[test]
    fn test_default_routing() {
        let config = RoutingConfig::default();
        let result = resolve_route(&config, "telegram", "user1", None);
        assert_eq!(result.agent_id, "default");
        assert!(result.matched_rule_id.is_none());
    }

    #[test]
    fn test_channel_match() {
        let config = RoutingConfig {
            rules: vec![
                make_rule("telegram", "research_agent", "Telegram → Research"),
                make_rule("discord", "creative_agent", "Discord → Creative"),
            ],
            default_agent_id: "default".into(),
        };

        let tg = resolve_route(&config, "telegram", "user1", None);
        assert_eq!(tg.agent_id, "research_agent");

        let dc = resolve_route(&config, "discord", "user2", None);
        assert_eq!(dc.agent_id, "creative_agent");

        let irc = resolve_route(&config, "irc", "user3", None);
        assert_eq!(irc.agent_id, "default");
    }

    #[test]
    fn test_user_filter() {
        let config = RoutingConfig {
            rules: vec![RoutingRule {
                id: "r1".into(),
                channel: "telegram".into(),
                user_filter: vec!["vip_user".into()],
                channel_id_filter: vec![],
                agent_id: "vip_agent".into(),
                label: "VIP Telegram".into(),
                enabled: true,
            }],
            default_agent_id: "default".into(),
        };

        let vip = resolve_route(&config, "telegram", "vip_user", None);
        assert_eq!(vip.agent_id, "vip_agent");

        let normal = resolve_route(&config, "telegram", "normal_user", None);
        assert_eq!(normal.agent_id, "default");
    }

    #[test]
    fn test_wildcard_rule() {
        let config = RoutingConfig {
            rules: vec![
                make_rule("discord", "discord_agent", "Discord specific"),
                make_rule("*", "catch_all_agent", "Catch-all"),
            ],
            default_agent_id: "default".into(),
        };

        let dc = resolve_route(&config, "discord", "u1", None);
        assert_eq!(dc.agent_id, "discord_agent");

        let tg = resolve_route(&config, "telegram", "u2", None);
        assert_eq!(tg.agent_id, "catch_all_agent");
    }

    #[test]
    fn test_disabled_rule_skipped() {
        let config = RoutingConfig {
            rules: vec![RoutingRule {
                enabled: false,
                ..make_rule("telegram", "disabled_agent", "Disabled")
            }],
            default_agent_id: "default".into(),
        };

        let result = resolve_route(&config, "telegram", "u1", None);
        assert_eq!(result.agent_id, "default");
    }
}
