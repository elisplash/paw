// Paw Agent Engine — Skill Vault
// Secure credential storage and skill management for agent tools.
//
// Architecture:
// - Skill definitions are built-in (email, slack, github, generic REST).
// - Credentials are stored encrypted in SQLite, with the encryption key in OS keychain.
// - The agent gets tools for enabled skills but NEVER sees raw credentials.
// - Credentials are injected at execution time by the tool executor.

use log::{info, warn};
use serde::{Deserialize, Serialize};

/// A skill definition — describes what the skill does and what credentials it needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    /// Credentials this skill requires (name → description).
    pub required_credentials: Vec<CredentialField>,
    /// The tool names this skill provides to the agent.
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub description: String,
    /// If true, this is a required field. If false, optional.
    pub required: bool,
    /// Hint text for the input field.
    pub placeholder: String,
}

/// A stored skill record (from DB) — tracks enabled state and credential status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub skill_id: String,
    pub enabled: bool,
    /// Which credential keys have been set (not the values — just the key names).
    pub configured_keys: Vec<String>,
    pub updated_at: String,
}

/// Skill status for the frontend — combines definition + stored state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub required_credentials: Vec<CredentialField>,
    pub configured_credentials: Vec<String>,
    pub missing_credentials: Vec<String>,
    pub is_ready: bool,
    pub tool_names: Vec<String>,
}

// ── Built-in Skill Definitions ─────────────────────────────────────────

pub fn builtin_skills() -> Vec<SkillDefinition> {
    vec![
        SkillDefinition {
            id: "email".into(),
            name: "Email".into(),
            description: "Send and read emails via IMAP/SMTP".into(),
            icon: "📧".into(),
            required_credentials: vec![
                CredentialField {
                    key: "SMTP_HOST".into(),
                    label: "SMTP Host".into(),
                    description: "SMTP server hostname (e.g. smtp.gmail.com)".into(),
                    required: true,
                    placeholder: "smtp.gmail.com".into(),
                },
                CredentialField {
                    key: "SMTP_PORT".into(),
                    label: "SMTP Port".into(),
                    description: "SMTP port (587 for TLS, 465 for SSL)".into(),
                    required: true,
                    placeholder: "587".into(),
                },
                CredentialField {
                    key: "SMTP_USER".into(),
                    label: "Email Address".into(),
                    description: "Your email address for authentication".into(),
                    required: true,
                    placeholder: "you@gmail.com".into(),
                },
                CredentialField {
                    key: "SMTP_PASSWORD".into(),
                    label: "App Password".into(),
                    description: "App-specific password (not your main password)".into(),
                    required: true,
                    placeholder: "xxxx xxxx xxxx xxxx".into(),
                },
                CredentialField {
                    key: "IMAP_HOST".into(),
                    label: "IMAP Host".into(),
                    description: "IMAP server for reading mail (e.g. imap.gmail.com)".into(),
                    required: false,
                    placeholder: "imap.gmail.com".into(),
                },
                CredentialField {
                    key: "IMAP_PORT".into(),
                    label: "IMAP Port".into(),
                    description: "IMAP port (993 for SSL)".into(),
                    required: false,
                    placeholder: "993".into(),
                },
            ],
            tool_names: vec!["email_send".into(), "email_read".into()],
        },
        SkillDefinition {
            id: "slack".into(),
            name: "Slack".into(),
            description: "Send messages to Slack channels and DMs".into(),
            icon: "💬".into(),
            required_credentials: vec![
                CredentialField {
                    key: "SLACK_BOT_TOKEN".into(),
                    label: "Bot Token".into(),
                    description: "Slack Bot User OAuth Token (xoxb-...)".into(),
                    required: true,
                    placeholder: "xoxb-your-slack-bot-token".into(),
                },
                CredentialField {
                    key: "SLACK_DEFAULT_CHANNEL".into(),
                    label: "Default Channel".into(),
                    description: "Default channel ID to post to (optional)".into(),
                    required: false,
                    placeholder: "C0123456789".into(),
                },
            ],
            tool_names: vec!["slack_send".into(), "slack_read".into()],
        },
        SkillDefinition {
            id: "github".into(),
            name: "GitHub".into(),
            description: "Create issues, PRs, read repos, manage projects".into(),
            icon: "🐙".into(),
            required_credentials: vec![
                CredentialField {
                    key: "GITHUB_TOKEN".into(),
                    label: "Personal Access Token".into(),
                    description: "GitHub PAT with repo access (ghp_...)".into(),
                    required: true,
                    placeholder: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into(),
                },
            ],
            tool_names: vec!["github_api".into()],
        },
        SkillDefinition {
            id: "rest_api".into(),
            name: "REST API".into(),
            description: "Make authenticated API calls to any REST service. Store API keys and base URLs for services you use frequently.".into(),
            icon: "🔌".into(),
            required_credentials: vec![
                CredentialField {
                    key: "API_BASE_URL".into(),
                    label: "Base URL".into(),
                    description: "The base URL for the API (e.g. https://api.example.com/v1)".into(),
                    required: true,
                    placeholder: "https://api.example.com/v1".into(),
                },
                CredentialField {
                    key: "API_KEY".into(),
                    label: "API Key".into(),
                    description: "Authentication key/token for the API".into(),
                    required: true,
                    placeholder: "sk-...".into(),
                },
                CredentialField {
                    key: "API_AUTH_HEADER".into(),
                    label: "Auth Header Name".into(),
                    description: "Header name for the API key (default: Authorization)".into(),
                    required: false,
                    placeholder: "Authorization".into(),
                },
                CredentialField {
                    key: "API_AUTH_PREFIX".into(),
                    label: "Auth Prefix".into(),
                    description: "Prefix before the key in the header (default: Bearer)".into(),
                    required: false,
                    placeholder: "Bearer".into(),
                },
            ],
            tool_names: vec!["rest_api_call".into()],
        },
        SkillDefinition {
            id: "webhook".into(),
            name: "Webhooks".into(),
            description: "Send data to webhook URLs (Zapier, IFTTT, n8n, custom)".into(),
            icon: "🪝".into(),
            required_credentials: vec![
                CredentialField {
                    key: "WEBHOOK_URL".into(),
                    label: "Webhook URL".into(),
                    description: "The webhook endpoint URL".into(),
                    required: true,
                    placeholder: "https://hooks.zapier.com/hooks/catch/...".into(),
                },
                CredentialField {
                    key: "WEBHOOK_SECRET".into(),
                    label: "Secret (optional)".into(),
                    description: "Shared secret for webhook signing".into(),
                    required: false,
                    placeholder: "whsec_...".into(),
                },
            ],
            tool_names: vec!["webhook_send".into()],
        },
    ]
}

// ── Credential Storage (in SessionStore) ───────────────────────────────

use crate::engine::sessions::SessionStore;

impl SessionStore {
    /// Initialize the skill vault tables (call from open()).
    pub fn init_skill_tables(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS skill_credentials (
                skill_id TEXT NOT NULL,
                cred_key TEXT NOT NULL,
                cred_value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (skill_id, cred_key)
            );

            CREATE TABLE IF NOT EXISTS skill_state (
                skill_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ").map_err(|e| format!("Failed to create skill tables: {}", e))?;
        Ok(())
    }

    /// Store a credential for a skill.
    /// Value is stored encrypted (caller must encrypt before calling).
    pub fn set_skill_credential(&self, skill_id: &str, key: &str, encrypted_value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO skill_credentials (skill_id, cred_key, cred_value, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(skill_id, cred_key) DO UPDATE SET cred_value = ?3, updated_at = datetime('now')",
            rusqlite::params![skill_id, key, encrypted_value],
        ).map_err(|e| format!("Set credential error: {}", e))?;
        Ok(())
    }

    /// Get a credential for a skill (returns encrypted value).
    pub fn get_skill_credential(&self, skill_id: &str, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let result = conn.query_row(
            "SELECT cred_value FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
            |row: &rusqlite::Row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Delete a credential for a skill.
    pub fn delete_skill_credential(&self, skill_id: &str, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
        ).map_err(|e| format!("Delete credential error: {}", e))?;
        Ok(())
    }

    /// Delete ALL credentials for a skill.
    pub fn delete_all_skill_credentials(&self, skill_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1",
            rusqlite::params![skill_id],
        ).map_err(|e| format!("Delete credentials error: {}", e))?;
        Ok(())
    }

    /// List which credential keys are set for a skill (not the values).
    pub fn list_skill_credential_keys(&self, skill_id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT cred_key FROM skill_credentials WHERE skill_id = ?1 ORDER BY cred_key"
        ).map_err(|e| format!("Prepare error: {}", e))?;
        let keys: Vec<String> = stmt.query_map(rusqlite::params![skill_id], |row: &rusqlite::Row| row.get::<_, String>(0))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r: Result<String, rusqlite::Error>| r.ok())
            .collect();
        Ok(keys)
    }

    /// Get/set skill enabled state.
    pub fn set_skill_enabled(&self, skill_id: &str, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO skill_state (skill_id, enabled, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(skill_id) DO UPDATE SET enabled = ?2, updated_at = datetime('now')",
            rusqlite::params![skill_id, enabled as i32],
        ).map_err(|e| format!("Set skill state error: {}", e))?;
        Ok(())
    }

    pub fn is_skill_enabled(&self, skill_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let result = conn.query_row(
            "SELECT enabled FROM skill_state WHERE skill_id = ?1",
            rusqlite::params![skill_id],
            |row: &rusqlite::Row| row.get::<_, i32>(0),
        );
        match result {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }
}

// ── Encryption helpers ─────────────────────────────────────────────────
// We use a simple XOR-with-key approach using a random key stored in the OS keychain.
// This isn't military-grade crypto but it keeps credentials from being readable
// if someone directly opens the SQLite file. The real security is the OS keychain.

const VAULT_KEYRING_SERVICE: &str = "paw-skill-vault";
const VAULT_KEYRING_USER: &str = "encryption-key";

/// Get or create the vault encryption key from the OS keychain.
pub fn get_vault_key() -> Result<Vec<u8>, String> {
    let entry = keyring::Entry::new(VAULT_KEYRING_SERVICE, VAULT_KEYRING_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;

    match entry.get_password() {
        Ok(key_b64) => {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64)
                .map_err(|e| format!("Failed to decode vault key: {}", e))
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a new random key
            use rand::Rng;
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill(&mut key[..]);
            let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &key);
            entry.set_password(&key_b64)
                .map_err(|e| format!("Failed to store vault key in keychain: {}", e))?;
            info!("[vault] Created new vault encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Encrypt a plaintext credential value.
pub fn encrypt_credential(plaintext: &str, key: &[u8]) -> String {
    let bytes = plaintext.as_bytes();
    let encrypted: Vec<u8> = bytes.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted)
}

/// Decrypt an encrypted credential value.
pub fn decrypt_credential(encrypted_b64: &str, key: &[u8]) -> Result<String, String> {
    let encrypted = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_b64)
        .map_err(|e| format!("Failed to decode: {}", e))?;
    let decrypted: Vec<u8> = encrypted.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect();
    String::from_utf8(decrypted).map_err(|e| format!("Failed to decrypt: {}", e))
}

// ── Skill Status Helpers ───────────────────────────────────────────────

/// Get the combined status of all skills (definition + stored state).
pub fn get_all_skill_status(store: &SessionStore) -> Result<Vec<SkillStatus>, String> {
    let definitions = builtin_skills();
    let mut statuses = Vec::new();

    for def in &definitions {
        let enabled = store.is_skill_enabled(&def.id)?;
        let configured_keys = store.list_skill_credential_keys(&def.id)?;
        let missing: Vec<String> = def.required_credentials.iter()
            .filter(|c| c.required && !configured_keys.contains(&c.key))
            .map(|c| c.key.clone())
            .collect();
        let is_ready = enabled && missing.is_empty();

        statuses.push(SkillStatus {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            icon: def.icon.clone(),
            enabled,
            required_credentials: def.required_credentials.clone(),
            configured_credentials: configured_keys,
            missing_credentials: missing,
            is_ready,
            tool_names: def.tool_names.clone(),
        });
    }

    Ok(statuses)
}

/// Get credential values for a skill (decrypted). Used by tool executor at runtime.
pub fn get_skill_credentials(store: &SessionStore, skill_id: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let vault_key = get_vault_key()?;
    let keys = store.list_skill_credential_keys(skill_id)?;
    let mut creds = std::collections::HashMap::new();

    for key in keys {
        if let Some(encrypted) = store.get_skill_credential(skill_id, &key)? {
            match decrypt_credential(&encrypted, &vault_key) {
                Ok(value) => { creds.insert(key, value); }
                Err(e) => {
                    warn!("[vault] Failed to decrypt {}:{}: {}", skill_id, key, e);
                }
            }
        }
    }

    Ok(creds)
}
