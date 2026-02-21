// Paw — Local SQLite Database
// Provides persistent storage for workspaces, projects, automations, modes, etc.

import Database from '@tauri-apps/plugin-sql';

let db: Database | null = null;

export async function initDb(): Promise<Database> {
  if (db) return db;
  db = await Database.load('sqlite:paw.db');
  await runMigrations(db);
  return db;
}

export function getDb(): Database | null {
  return db;
}

// ── Field-level encryption (C2) ────────────────────────────────────────────
// Uses Web Crypto API with AES-GCM for encrypting sensitive fields before
// storing in SQLite. The 256-bit key is derived from the OS keychain via
// the Rust get_db_encryption_key command.

let _cryptoKey: CryptoKey | null = null;
const ENC_PREFIX = 'enc:'; // marker prefix for encrypted values

/**
 * Initialise the encryption key from the OS keychain (via Tauri invoke).
 * Call once after Tauri is ready. No-op in browser mode.
 */
export async function initDbEncryption(): Promise<boolean> {
  try {
    const invoke = (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
      ? (await import('@tauri-apps/api/core')).invoke
      : null;
    if (!invoke) return false;

    const hexKey = await invoke<string>('get_db_encryption_key');
    if (!hexKey || hexKey.length < 32) {
      console.error(
        '[db] OS keychain returned invalid encryption key — credential storage will be blocked',
      );
      return false;
    }

    // Convert hex string to raw bytes
    const hexPairs = hexKey.match(/.{1,2}/g);
    if (!hexPairs) return false;
    const keyBytes = new Uint8Array(hexPairs.map((b) => parseInt(b, 16)));
    _cryptoKey = await crypto.subtle.importKey('raw', keyBytes, { name: 'AES-GCM' }, false, [
      'encrypt',
      'decrypt',
    ]);
    console.debug('[db] Encryption key loaded from OS keychain');
    return true;
  } catch (e) {
    console.error(
      '[db] OS keychain unavailable — encryption disabled, credential storage blocked:',
      e,
    );
    return false;
  }
}

/**
 * Encrypt a plaintext string. Returns "enc:<base64(iv+ciphertext)>".
 * Throws if encryption isn't initialised — callers must handle the error
 * and must NOT store sensitive data as plaintext.
 */
export async function encryptField(plaintext: string): Promise<string> {
  if (!_cryptoKey) {
    console.error(
      '[db] encryptField blocked — OS keychain unavailable. Refusing to store plaintext.',
    );
    throw new Error(
      'Encryption unavailable — OS keychain is not accessible. Cannot store sensitive data.',
    );
  }
  try {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const encoded = new TextEncoder().encode(plaintext);
    const cipher = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, _cryptoKey, encoded);
    // Concatenate IV + ciphertext and base64-encode
    const combined = new Uint8Array(iv.length + cipher.byteLength);
    combined.set(iv);
    combined.set(new Uint8Array(cipher), iv.length);
    return ENC_PREFIX + btoa(String.fromCharCode(...combined));
  } catch (e) {
    console.error('[db] AES-GCM encryption failed:', e);
    throw new Error(`Encryption failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

/**
 * Decrypt a value. If it starts with "enc:", decrypt it. Otherwise return as-is.
 */
export async function decryptField(stored: string): Promise<string> {
  if (!stored.startsWith(ENC_PREFIX) || !_cryptoKey) return stored;
  try {
    const b64 = stored.slice(ENC_PREFIX.length);
    const combined = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const iv = combined.slice(0, 12);
    const ciphertext = combined.slice(12);
    const decrypted = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, _cryptoKey, ciphertext);
    return new TextDecoder().decode(decrypted);
  } catch (e) {
    console.warn('[db] Decryption failed:', e);
    return stored; // return raw value if decryption fails
  }
}

/** Check if encryption is available (key loaded). */
export function isEncryptionReady(): boolean {
  return _cryptoKey !== null;
}

// ── Schema migration SQL statements ────────────────────────────────────────

/**
 * Versioned migrations. Each entry is [version, description, ...sql_statements].
 * Migrations are run in order; only those with version > current schema_version
 * are executed. Each migration runs inside a transaction.
 *
 * Rules for adding new migrations:
 *   1. Append a new entry with the next sequential version number.
 *   2. Never modify or reorder existing migrations.
 *   3. Use IF NOT EXISTS / IF EXISTS guards where appropriate.
 */
interface Migration {
  version: number;
  description: string;
  statements: string[];
}

const MIGRATIONS: Migration[] = [
  {
    version: 1,
    description: 'Initial schema — core tables',
    statements: [
      `CREATE TABLE IF NOT EXISTS agent_modes (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        model TEXT,
        system_prompt TEXT,
        skills TEXT, -- JSON array
        thinking_level TEXT DEFAULT 'normal',
        temperature REAL DEFAULT 1.0,
        icon TEXT DEFAULT '',
        color TEXT DEFAULT '#0073EA',
        is_default INTEGER DEFAULT 0,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now'))
      )`,
      `CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        space TEXT NOT NULL, -- 'build', 'research', 'create'
        description TEXT,
        session_key TEXT,
        metadata TEXT, -- JSON
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now'))
      )`,
      `CREATE TABLE IF NOT EXISTS project_files (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        path TEXT NOT NULL,
        content TEXT,
        language TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      )`,
      `CREATE TABLE IF NOT EXISTS automation_runs (
        id TEXT PRIMARY KEY,
        job_id TEXT NOT NULL,
        started_at TEXT,
        finished_at TEXT,
        status TEXT DEFAULT 'running',
        output TEXT,
        error TEXT
      )`,
      `CREATE TABLE IF NOT EXISTS research_findings (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        title TEXT,
        content TEXT,
        source_url TEXT,
        source_title TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      )`,
      `CREATE TABLE IF NOT EXISTS content_documents (
        id TEXT PRIMARY KEY,
        project_id TEXT,
        title TEXT NOT NULL,
        content TEXT DEFAULT '',
        content_type TEXT DEFAULT 'markdown', -- markdown, html, plaintext
        word_count INTEGER DEFAULT 0,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now'))
      )`,
      `CREATE TABLE IF NOT EXISTS email_accounts (
        id TEXT PRIMARY KEY,
        email TEXT NOT NULL,
        display_name TEXT,
        imap_host TEXT,
        imap_port INTEGER DEFAULT 993,
        smtp_host TEXT,
        smtp_port INTEGER DEFAULT 587,
        connected INTEGER DEFAULT 0,
        created_at TEXT DEFAULT (datetime('now'))
      )`,
      `CREATE TABLE IF NOT EXISTS emails (
        id TEXT PRIMARY KEY,
        account_id TEXT NOT NULL,
        message_id TEXT,
        folder TEXT DEFAULT 'inbox',
        from_addr TEXT,
        from_name TEXT,
        to_addr TEXT,
        subject TEXT,
        body_text TEXT,
        body_html TEXT,
        date TEXT,
        is_read INTEGER DEFAULT 0,
        is_starred INTEGER DEFAULT 0,
        agent_draft TEXT, -- AI-drafted reply
        agent_draft_status TEXT, -- 'pending', 'approved', 'sent'
        created_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (account_id) REFERENCES email_accounts(id) ON DELETE CASCADE
      )`,
      `CREATE TABLE IF NOT EXISTS credential_activity_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT DEFAULT (datetime('now')),
        account_name TEXT,
        action TEXT NOT NULL,
        tool_name TEXT,
        detail TEXT,
        session_key TEXT,
        was_allowed INTEGER DEFAULT 1
      )`,
      `CREATE TABLE IF NOT EXISTS security_audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT DEFAULT (datetime('now')),
        event_type TEXT NOT NULL,
        risk_level TEXT,
        tool_name TEXT,
        command TEXT,
        detail TEXT,
        session_key TEXT,
        was_allowed INTEGER DEFAULT 1,
        matched_pattern TEXT
      )`,
      `CREATE TABLE IF NOT EXISTS security_rules (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        rule_type TEXT NOT NULL,
        pattern TEXT NOT NULL,
        description TEXT,
        enabled INTEGER DEFAULT 1,
        created_at TEXT DEFAULT (datetime('now'))
      )`,
      `CREATE TABLE IF NOT EXISTS security_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        settings_json TEXT NOT NULL DEFAULT '{}'
      )`,
    ],
  },
  // ── Future migrations go here ──
  // {
  //   version: 3,
  //   description: 'Add foo column to bar table',
  //   statements: [
  //     `ALTER TABLE bar ADD COLUMN foo TEXT DEFAULT ''`,
  //   ],
  // },
  {
    version: 2,
    description: 'Model pricing overrides table',
    statements: [
      `CREATE TABLE IF NOT EXISTS model_pricing (
        model_key TEXT PRIMARY KEY,
        context_size INTEGER,
        cost_input REAL,
        cost_output REAL,
        updated_at TEXT DEFAULT (datetime('now'))
      )`,
    ],
  },
];

/** Seed default agent modes if the table is empty */
async function seedDefaultModes(db: Database): Promise<void> {
  const modes = await db.select<{ count: number }[]>('SELECT COUNT(*) as count FROM agent_modes');
  if (modes[0]?.count === 0) {
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color, is_default) VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ['default', 'General', null, '', 'G', '#0073EA', 1],
    );
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color) VALUES (?, ?, ?, ?, ?, ?)`,
      [
        'code-review',
        'Code Review',
        null,
        'You are a careful code reviewer. Focus on bugs, security issues, and performance problems. Be thorough and specific.',
        'CR',
        '#A25DDC',
      ],
    );
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color) VALUES (?, ?, ?, ?, ?, ?)`,
      [
        'fast-chat',
        'Quick Chat',
        null,
        'Be concise and direct. Short answers preferred.',
        'QC',
        '#FDAB3D',
      ],
    );
  }
}

async function runMigrations(db: Database) {
  // Ensure schema_version table exists (bootstrap — not versioned itself)
  await db.execute(`CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    description TEXT,
    applied_at TEXT DEFAULT (datetime('now'))
  )`);

  // Get current version
  const rows = await db.select<{ version: number }[]>(
    'SELECT COALESCE(MAX(version), 0) as version FROM schema_version',
  );
  const currentVersion = rows[0]?.version ?? 0;

  // Run pending migrations in order
  for (const migration of MIGRATIONS) {
    if (migration.version <= currentVersion) continue;

    console.debug(`[db] Running migration v${migration.version}: ${migration.description}`);
    try {
      // Wrap each migration in a transaction for atomicity
      await db.execute('BEGIN TRANSACTION');
      for (const sql of migration.statements) {
        await db.execute(sql);
      }
      await db.execute('INSERT INTO schema_version (version, description) VALUES (?, ?)', [
        migration.version,
        migration.description,
      ]);
      await db.execute('COMMIT');
    } catch (e) {
      await db.execute('ROLLBACK').catch(() => {});
      throw new Error(
        `Migration v${migration.version} failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }

  await seedDefaultModes(db);
}

// ── Agent Modes CRUD ─────────────────────────────────────────────────────

export interface AgentMode {
  id: string;
  name: string;
  model: string | null;
  system_prompt: string;
  skills: string;
  thinking_level: string;
  temperature: number;
  icon: string;
  color: string;
  is_default: number;
  created_at: string;
  updated_at: string;
}

export async function listModes(): Promise<AgentMode[]> {
  if (!db) return [];
  return db.select<AgentMode[]>('SELECT * FROM agent_modes ORDER BY is_default DESC, name ASC');
}

export async function getMode(id: string): Promise<AgentMode | null> {
  if (!db) return null;
  const rows = await db.select<AgentMode[]>('SELECT * FROM agent_modes WHERE id = ?', [id]);
  return rows[0] ?? null;
}

export async function saveMode(
  mode: Partial<AgentMode> & { id: string; name: string },
): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO agent_modes (id, name, model, system_prompt, skills, thinking_level, temperature, icon, color, is_default, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
     ON CONFLICT(id) DO UPDATE SET
       name            = excluded.name,
       model           = excluded.model,
       system_prompt   = excluded.system_prompt,
       skills          = excluded.skills,
       thinking_level  = excluded.thinking_level,
       temperature     = excluded.temperature,
       icon            = excluded.icon,
       color           = excluded.color,
       is_default      = excluded.is_default,
       updated_at      = datetime('now')`,
    [
      mode.id,
      mode.name,
      mode.model ?? null,
      mode.system_prompt ?? '',
      mode.skills ?? '[]',
      mode.thinking_level ?? 'normal',
      mode.temperature ?? 1.0,
      mode.icon ?? '',
      mode.color ?? '#0073EA',
      mode.is_default ?? 0,
    ],
  );
}

export async function deleteMode(id: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM agent_modes WHERE id = ? AND is_default = 0', [id]);
}

// ── Projects CRUD ─────────────────────────────────────────────────────────

export interface Project {
  id: string;
  name: string;
  space: string;
  description: string;
  session_key: string | null;
  metadata: string;
  created_at: string;
  updated_at: string;
}

export async function listProjects(space: string): Promise<Project[]> {
  if (!db) return [];
  return db.select<Project[]>('SELECT * FROM projects WHERE space = ? ORDER BY updated_at DESC', [
    space,
  ]);
}

export async function saveProject(
  proj: Partial<Project> & { id: string; name: string; space: string },
): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO projects (id, name, space, description, session_key, metadata, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
     ON CONFLICT(id) DO UPDATE SET
       name        = excluded.name,
       space       = excluded.space,
       description = excluded.description,
       session_key = excluded.session_key,
       metadata    = excluded.metadata,
       updated_at  = datetime('now')`,
    [
      proj.id,
      proj.name,
      proj.space,
      proj.description ?? '',
      proj.session_key ?? null,
      proj.metadata ?? '{}',
    ],
  );
}

export async function deleteProject(id: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM projects WHERE id = ?', [id]);
}

// ── Content Documents ──────────────────────────────────────────────────────

export interface ContentDoc {
  id: string;
  project_id: string | null;
  title: string;
  content: string;
  content_type: string;
  word_count: number;
  created_at: string;
  updated_at: string;
}

export async function listDocs(): Promise<ContentDoc[]> {
  if (!db) return [];
  return db.select<ContentDoc[]>('SELECT * FROM content_documents ORDER BY updated_at DESC');
}

export async function getDoc(id: string): Promise<ContentDoc | null> {
  if (!db) return null;
  const rows = await db.select<ContentDoc[]>('SELECT * FROM content_documents WHERE id = ?', [id]);
  return rows[0] ?? null;
}

export async function saveDoc(
  doc: Partial<ContentDoc> & { id: string; title: string },
): Promise<void> {
  if (!db) return;
  const wordCount = (doc.content ?? '').split(/\s+/).filter(Boolean).length;
  await db.execute(
    `INSERT INTO content_documents (id, project_id, title, content, content_type, word_count, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
     ON CONFLICT(id) DO UPDATE SET
       project_id   = excluded.project_id,
       title        = excluded.title,
       content      = excluded.content,
       content_type = excluded.content_type,
       word_count   = excluded.word_count,
       updated_at   = datetime('now')`,
    [
      doc.id,
      doc.project_id ?? null,
      doc.title,
      doc.content ?? '',
      doc.content_type ?? 'markdown',
      wordCount,
    ],
  );
}

export async function deleteDoc(id: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM content_documents WHERE id = ?', [id]);
}

// ── Model Pricing CRUD ────────────────────────────────────────────────────

export interface ModelPricingRow {
  model_key: string;
  context_size: number | null;
  cost_input: number | null;
  cost_output: number | null;
}

export async function listModelPricing(): Promise<ModelPricingRow[]> {
  if (!db) return [];
  return db.select<ModelPricingRow[]>(
    'SELECT model_key, context_size, cost_input, cost_output FROM model_pricing ORDER BY model_key',
  );
}

export async function upsertModelPricing(row: ModelPricingRow): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO model_pricing (model_key, context_size, cost_input, cost_output, updated_at)
     VALUES (?, ?, ?, ?, datetime('now'))
     ON CONFLICT(model_key) DO UPDATE SET
       context_size = excluded.context_size,
       cost_input   = excluded.cost_input,
       cost_output  = excluded.cost_output,
       updated_at   = datetime('now')`,
    [row.model_key, row.context_size, row.cost_input, row.cost_output],
  );
}

export async function deleteModelPricing(modelKey: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM model_pricing WHERE model_key = ?', [modelKey]);
}

// ── Project Files ──────────────────────────────────────────────────────────

export interface ProjectFile {
  id: string;
  project_id: string;
  path: string;
  content: string;
  language: string | null;
  created_at: string;
  updated_at: string;
}

export async function listProjectFiles(projectId: string): Promise<ProjectFile[]> {
  if (!db) return [];
  return db.select<ProjectFile[]>(
    'SELECT * FROM project_files WHERE project_id = ? ORDER BY path ASC',
    [projectId],
  );
}

export async function saveProjectFile(file: {
  id: string;
  project_id: string;
  path: string;
  content: string;
  language?: string;
}): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO project_files (id, project_id, path, content, language, updated_at)
     VALUES (?, ?, ?, ?, ?, datetime('now'))
     ON CONFLICT(id) DO UPDATE SET
       project_id = excluded.project_id,
       path       = excluded.path,
       content    = excluded.content,
       language   = excluded.language,
       updated_at = datetime('now')`,
    [file.id, file.project_id, file.path, file.content, file.language ?? null],
  );
}

export async function deleteProjectFile(id: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM project_files WHERE id = ?', [id]);
}

// ── Credential Activity Log ───────────────────────────────────────────────

export interface CredentialLogEntry {
  id: number;
  timestamp: string;
  account_name: string | null;
  action: string;
  tool_name: string | null;
  detail: string | null;
  session_key: string | null;
  was_allowed: number;
}

export async function logCredentialActivity(entry: {
  accountName?: string;
  action: string;
  toolName?: string;
  detail?: string;
  sessionKey?: string;
  wasAllowed?: boolean;
}): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO credential_activity_log (account_name, action, tool_name, detail, session_key, was_allowed)
     VALUES (?, ?, ?, ?, ?, ?)`,
    [
      entry.accountName ?? null,
      entry.action,
      entry.toolName ?? null,
      entry.detail ?? null,
      entry.sessionKey ?? null,
      entry.wasAllowed !== false ? 1 : 0,
    ],
  );
}

export async function getCredentialActivityLog(limit = 50): Promise<CredentialLogEntry[]> {
  if (!db) return [];
  return db.select<CredentialLogEntry[]>(
    'SELECT * FROM credential_activity_log ORDER BY id DESC LIMIT ?',
    [limit],
  );
}
// ── Security Audit Log ────────────────────────────────────────────────────

export interface SecurityAuditEntry {
  id: number;
  timestamp: string;
  event_type: string;
  risk_level: string | null;
  tool_name: string | null;
  command: string | null;
  detail: string | null;
  session_key: string | null;
  was_allowed: number;
  matched_pattern: string | null;
}

export async function logSecurityEvent(entry: {
  eventType: string;
  riskLevel?: string | null;
  toolName?: string;
  command?: string;
  detail?: string;
  sessionKey?: string;
  wasAllowed?: boolean;
  matchedPattern?: string;
}): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO security_audit_log (event_type, risk_level, tool_name, command, detail, session_key, was_allowed, matched_pattern)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      entry.eventType,
      entry.riskLevel ?? null,
      entry.toolName ?? null,
      entry.command ?? null,
      entry.detail ?? null,
      entry.sessionKey ?? null,
      entry.wasAllowed !== false ? 1 : 0,
      entry.matchedPattern ?? null,
    ],
  );
}

export async function getSecurityAuditLog(
  limit = 100,
  eventType?: string,
): Promise<SecurityAuditEntry[]> {
  if (!db) return [];
  if (eventType) {
    return db.select<SecurityAuditEntry[]>(
      'SELECT * FROM security_audit_log WHERE event_type = ? ORDER BY id DESC LIMIT ?',
      [eventType, limit],
    );
  }
  return db.select<SecurityAuditEntry[]>(
    'SELECT * FROM security_audit_log ORDER BY id DESC LIMIT ?',
    [limit],
  );
}

// ── Security Rules CRUD ───────────────────────────────────────────────────

export interface SecurityRule {
  id: number;
  rule_type: string;
  pattern: string;
  description: string | null;
  enabled: number;
  created_at: string;
}

export async function listSecurityRules(ruleType?: string): Promise<SecurityRule[]> {
  if (!db) return [];
  if (ruleType) {
    return db.select<SecurityRule[]>(
      'SELECT * FROM security_rules WHERE rule_type = ? ORDER BY id ASC',
      [ruleType],
    );
  }
  return db.select<SecurityRule[]>('SELECT * FROM security_rules ORDER BY rule_type, id ASC');
}

export async function addSecurityRule(rule: {
  ruleType: string;
  pattern: string;
  description?: string;
}): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO security_rules (rule_type, pattern, description) VALUES (?, ?, ?)`,
    [rule.ruleType, rule.pattern, rule.description ?? null],
  );
}

export async function removeSecurityRule(id: number): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM security_rules WHERE id = ?', [id]);
}

export async function toggleSecurityRule(id: number, enabled: boolean): Promise<void> {
  if (!db) return;
  await db.execute('UPDATE security_rules SET enabled = ? WHERE id = ?', [enabled ? 1 : 0, id]);
}

// ── Security Settings (encrypted DB storage) ──────────────────────────────

/**
 * Load the raw security settings JSON from the database.
 * Returns the parsed object or null if no row exists.
 */
export async function loadSecuritySettingsFromDb(): Promise<Record<string, unknown> | null> {
  if (!db) return null;
  const rows = await db.select<{ settings_json: string }[]>(
    'SELECT settings_json FROM security_settings WHERE id = 1',
  );
  if (rows.length === 0) return null;
  try {
    return JSON.parse(rows[0].settings_json);
  } catch {
    return null;
  }
}

/**
 * Save security settings JSON to the database (upsert).
 */
export async function saveSecuritySettingsToDb(settingsJson: string): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO security_settings (id, settings_json) VALUES (1, ?)
     ON CONFLICT(id) DO UPDATE SET settings_json = excluded.settings_json`,
    [settingsJson],
  );
}

/**
 * Delete the security settings row (for reset to defaults).
 */
export async function resetSecuritySettingsInDb(): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM security_settings WHERE id = 1');
}
