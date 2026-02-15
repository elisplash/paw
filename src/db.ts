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

async function runMigrations(db: Database) {
  await db.execute(`
    CREATE TABLE IF NOT EXISTS agent_modes (
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
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS projects (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      space TEXT NOT NULL, -- 'build', 'research', 'create'
      description TEXT,
      session_key TEXT,
      metadata TEXT, -- JSON
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS project_files (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      path TEXT NOT NULL,
      content TEXT,
      language TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS automation_runs (
      id TEXT PRIMARY KEY,
      job_id TEXT NOT NULL,
      started_at TEXT,
      finished_at TEXT,
      status TEXT DEFAULT 'running',
      output TEXT,
      error TEXT
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS research_findings (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      title TEXT,
      content TEXT,
      source_url TEXT,
      source_title TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS content_documents (
      id TEXT PRIMARY KEY,
      project_id TEXT,
      title TEXT NOT NULL,
      content TEXT DEFAULT '',
      content_type TEXT DEFAULT 'markdown', -- markdown, html, plaintext
      word_count INTEGER DEFAULT 0,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS email_accounts (
      id TEXT PRIMARY KEY,
      email TEXT NOT NULL,
      display_name TEXT,
      imap_host TEXT,
      imap_port INTEGER DEFAULT 993,
      smtp_host TEXT,
      smtp_port INTEGER DEFAULT 587,
      connected INTEGER DEFAULT 0,
      created_at TEXT DEFAULT (datetime('now'))
    )
  `);

  await db.execute(`
    CREATE TABLE IF NOT EXISTS emails (
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
    )
  `);

  // Credential activity log — every agent action involving credentials
  await db.execute(`
    CREATE TABLE IF NOT EXISTS credential_activity_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      timestamp TEXT DEFAULT (datetime('now')),
      account_name TEXT,
      action TEXT NOT NULL,        -- 'read', 'send', 'delete', 'manage', 'blocked', 'approved', 'denied'
      tool_name TEXT,              -- e.g. 'himalaya envelope list', 'himalaya send'
      detail TEXT,                 -- human-readable description
      session_key TEXT,
      was_allowed INTEGER DEFAULT 1  -- 0 = blocked by permission policy
    )
  `);

  // Unified security audit log — all security-relevant events in one place
  await db.execute(`
    CREATE TABLE IF NOT EXISTS security_audit_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      timestamp TEXT DEFAULT (datetime('now')),
      event_type TEXT NOT NULL,     -- 'exec_approval', 'credential_access', 'skill_install', 'config_change', 'auto_deny', 'auto_allow', 'security_policy'
      risk_level TEXT,              -- 'critical', 'high', 'medium', 'low', 'safe', null
      tool_name TEXT,
      command TEXT,                 -- the raw command or tool invocation
      detail TEXT,                  -- human-readable description
      session_key TEXT,
      was_allowed INTEGER DEFAULT 1,
      matched_pattern TEXT          -- which security rule or pattern triggered
    )
  `);

  // Command security rules — user-defined allowlist/denylist patterns
  await db.execute(`
    CREATE TABLE IF NOT EXISTS security_rules (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      rule_type TEXT NOT NULL,      -- 'allow' or 'deny'
      pattern TEXT NOT NULL,        -- regex pattern
      description TEXT,             -- optional human label
      enabled INTEGER DEFAULT 1,
      created_at TEXT DEFAULT (datetime('now'))
    )
  `);

  // Seed default agent mode if none exist
  const modes = await db.select<{ count: number }[]>('SELECT COUNT(*) as count FROM agent_modes');
  if (modes[0]?.count === 0) {
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color, is_default) VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ['default', 'General', null, '', 'G', '#0073EA', 1]
    );
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color) VALUES (?, ?, ?, ?, ?, ?)`,
      ['code-review', 'Code Review', null, 'You are a careful code reviewer. Focus on bugs, security issues, and performance problems. Be thorough and specific.', 'CR', '#A25DDC']
    );
    await db.execute(
      `INSERT INTO agent_modes (id, name, model, system_prompt, icon, color) VALUES (?, ?, ?, ?, ?, ?)`,
      ['fast-chat', 'Quick Chat', null, 'Be concise and direct. Short answers preferred.', 'QC', '#FDAB3D']
    );
  }
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

export async function saveMode(mode: Partial<AgentMode> & { id: string; name: string }): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT OR REPLACE INTO agent_modes (id, name, model, system_prompt, skills, thinking_level, temperature, icon, color, is_default, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))`,
    [mode.id, mode.name, mode.model ?? null, mode.system_prompt ?? '', mode.skills ?? '[]',
     mode.thinking_level ?? 'normal', mode.temperature ?? 1.0, mode.icon ?? '',
     mode.color ?? '#0073EA', mode.is_default ?? 0]
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
  return db.select<Project[]>('SELECT * FROM projects WHERE space = ? ORDER BY updated_at DESC', [space]);
}

export async function saveProject(proj: Partial<Project> & { id: string; name: string; space: string }): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT OR REPLACE INTO projects (id, name, space, description, session_key, metadata, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, datetime('now'))`,
    [proj.id, proj.name, proj.space, proj.description ?? '', proj.session_key ?? null, proj.metadata ?? '{}']
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

export async function saveDoc(doc: Partial<ContentDoc> & { id: string; title: string }): Promise<void> {
  if (!db) return;
  const wordCount = (doc.content ?? '').split(/\s+/).filter(Boolean).length;
  await db.execute(
    `INSERT OR REPLACE INTO content_documents (id, project_id, title, content, content_type, word_count, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, datetime('now'))`,
    [doc.id, doc.project_id ?? null, doc.title, doc.content ?? '', doc.content_type ?? 'markdown', wordCount]
  );
}

export async function deleteDoc(id: string): Promise<void> {
  if (!db) return;
  await db.execute('DELETE FROM content_documents WHERE id = ?', [id]);
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
  return db.select<ProjectFile[]>('SELECT * FROM project_files WHERE project_id = ? ORDER BY path ASC', [projectId]);
}

export async function saveProjectFile(file: { id: string; project_id: string; path: string; content: string; language?: string }): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT OR REPLACE INTO project_files (id, project_id, path, content, language, updated_at)
     VALUES (?, ?, ?, ?, ?, datetime('now'))`,
    [file.id, file.project_id, file.path, file.content, file.language ?? null]
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
    [entry.accountName ?? null, entry.action, entry.toolName ?? null, entry.detail ?? null,
     entry.sessionKey ?? null, entry.wasAllowed !== false ? 1 : 0]
  );
}

export async function getCredentialActivityLog(limit = 50): Promise<CredentialLogEntry[]> {
  if (!db) return [];
  return db.select<CredentialLogEntry[]>(
    'SELECT * FROM credential_activity_log ORDER BY id DESC LIMIT ?', [limit]
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
    ]
  );
}

export async function getSecurityAuditLog(limit = 100, eventType?: string): Promise<SecurityAuditEntry[]> {
  if (!db) return [];
  if (eventType) {
    return db.select<SecurityAuditEntry[]>(
      'SELECT * FROM security_audit_log WHERE event_type = ? ORDER BY id DESC LIMIT ?', [eventType, limit]
    );
  }
  return db.select<SecurityAuditEntry[]>(
    'SELECT * FROM security_audit_log ORDER BY id DESC LIMIT ?', [limit]
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
      'SELECT * FROM security_rules WHERE rule_type = ? ORDER BY id ASC', [ruleType]
    );
  }
  return db.select<SecurityRule[]>('SELECT * FROM security_rules ORDER BY rule_type, id ASC');
}

export async function addSecurityRule(rule: { ruleType: string; pattern: string; description?: string }): Promise<void> {
  if (!db) return;
  await db.execute(
    `INSERT INTO security_rules (rule_type, pattern, description) VALUES (?, ?, ?)`,
    [rule.ruleType, rule.pattern, rule.description ?? null]
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