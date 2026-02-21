// Paw — Security: Dangerous Command Classifier & Policy Engine
// Classifies exec approval requests by risk level and enforces command policies.

// ── Risk levels ────────────────────────────────────────────────────────────

export type RiskLevel = 'critical' | 'high' | 'medium' | 'low' | 'safe';

export interface RiskClassification {
  level: RiskLevel;
  label: string;
  reason: string; // human-readable explanation
  matchedPattern: string; // the pattern that triggered
}

// ── Pattern definitions ────────────────────────────────────────────────────

interface DangerPattern {
  pattern: RegExp;
  level: RiskLevel;
  label: string;
  reason: string;
}

const DANGER_PATTERNS: DangerPattern[] = [
  // ── CRITICAL: Privilege escalation ──
  {
    pattern: /\bsudo\b/i,
    level: 'critical',
    label: 'Privilege Escalation',
    reason: 'Uses sudo to run commands as root',
  },
  {
    pattern: /\bsu\s+(-|root|\w)/i,
    level: 'critical',
    label: 'Privilege Escalation',
    reason: 'Switches to another user (su)',
  },
  {
    pattern: /\bdoas\b/i,
    level: 'critical',
    label: 'Privilege Escalation',
    reason: 'Uses doas to run commands as root',
  },
  {
    pattern: /\bpkexec\b/i,
    level: 'critical',
    label: 'Privilege Escalation',
    reason: 'Uses pkexec for privilege escalation',
  },
  {
    pattern: /\brunas\b/i,
    level: 'critical',
    label: 'Privilege Escalation',
    reason: 'Uses runas to run as another user',
  },

  // ── CRITICAL: Destructive deletion ──
  {
    pattern:
      /\brm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*r[a-zA-Z]*\s+)?|(-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*f[a-zA-Z]*\s+)?))[\/"'~*]/i,
    level: 'critical',
    label: 'Destructive Deletion',
    reason: 'Recursive forced deletion targeting root, home, or wildcard paths',
  },
  {
    pattern: /\brm\s+-rf\s*\//i,
    level: 'critical',
    label: 'Destructive Deletion',
    reason: 'rm -rf / — destroys the entire filesystem',
  },
  {
    pattern: /\brm\s+-rf\s+~/i,
    level: 'critical',
    label: 'Destructive Deletion',
    reason: 'rm -rf ~ — destroys the home directory',
  },

  // ── CRITICAL: Disk destruction ──
  {
    pattern: /\bdd\s+if=/i,
    level: 'critical',
    label: 'Disk Write',
    reason: 'dd can overwrite disk partitions or devices',
  },
  {
    pattern: /\bmkfs\b/i,
    level: 'critical',
    label: 'Disk Format',
    reason: 'mkfs formats a disk partition',
  },
  {
    pattern: /\bfdisk\b/i,
    level: 'critical',
    label: 'Disk Partition',
    reason: 'fdisk modifies disk partitions',
  },
  {
    pattern: />\s*\/dev\/sd/i,
    level: 'critical',
    label: 'Device Write',
    reason: 'Writing directly to a block device',
  },

  // ── CRITICAL: Fork bomb ──
  {
    pattern: /:\(\)\s*\{.*\|.*&\s*\}\s*;?\s*:/,
    level: 'critical',
    label: 'Fork Bomb',
    reason: 'Shell fork bomb — will crash the system',
  },

  // ── CRITICAL: Remote code execution ──
  {
    pattern: /\bcurl\b.*\|\s*(ba)?sh/i,
    level: 'critical',
    label: 'Remote Code Exec',
    reason: 'Downloads and executes remote script (curl | sh)',
  },
  {
    pattern: /\bwget\b.*\|\s*(ba)?sh/i,
    level: 'critical',
    label: 'Remote Code Exec',
    reason: 'Downloads and executes remote script (wget | sh)',
  },
  {
    pattern: /\bcurl\b.*\|\s*python/i,
    level: 'critical',
    label: 'Remote Code Exec',
    reason: 'Downloads and pipes to python interpreter',
  },
  {
    pattern: /\bwget\b.*\|\s*python/i,
    level: 'critical',
    label: 'Remote Code Exec',
    reason: 'Downloads and pipes to python interpreter',
  },

  // ── HIGH: Firewall / network security ──
  {
    pattern: /\biptables\s+-F/i,
    level: 'high',
    label: 'Firewall Flush',
    reason: 'Flushes all iptables/firewall rules',
  },
  {
    pattern: /\bufw\s+disable/i,
    level: 'high',
    label: 'Firewall Disable',
    reason: 'Disables the UFW firewall',
  },
  {
    pattern: /\bfirewalld?\b.*stop/i,
    level: 'high',
    label: 'Firewall Stop',
    reason: 'Stops the firewall daemon',
  },

  // ── HIGH: User/account modification ──
  {
    pattern: /\bpasswd\b/i,
    level: 'high',
    label: 'Password Change',
    reason: 'Modifies user passwords',
  },
  {
    pattern: /\bchpasswd\b/i,
    level: 'high',
    label: 'Password Change',
    reason: 'Batch modifies user passwords',
  },
  {
    pattern: /\busermod\b/i,
    level: 'high',
    label: 'User Modification',
    reason: 'Modifies user account properties',
  },
  {
    pattern: /\buseradd\b/i,
    level: 'high',
    label: 'User Creation',
    reason: 'Creates a new user account',
  },
  {
    pattern: /\buserdel\b/i,
    level: 'high',
    label: 'User Deletion',
    reason: 'Deletes a user account',
  },

  // ── HIGH: Process killing ──
  {
    pattern: /\bkill\s+-9\s+1\b/i,
    level: 'high',
    label: 'Kill Init',
    reason: 'Sends SIGKILL to PID 1 (init)',
  },
  {
    pattern: /\bkillall\b/i,
    level: 'high',
    label: 'Kill All Processes',
    reason: 'Kills all processes matching a name',
  },

  // ── HIGH: Cron / scheduled task destruction ──
  {
    pattern: /\bcrontab\s+-r\b/i,
    level: 'high',
    label: 'Cron Wipe',
    reason: 'Removes all crontab entries',
  },

  // ── HIGH: SSH key destruction ──
  {
    pattern: /\bssh-keygen\b.*-f/i,
    level: 'high',
    label: 'SSH Key Overwrite',
    reason: 'May overwrite existing SSH keys',
  },

  // ── MEDIUM: Permission changes ──
  {
    pattern: /\bchmod\s+(777|a\+rwx)/i,
    level: 'medium',
    label: 'Permission Exposure',
    reason: 'Sets world-readable/writable permissions (777)',
  },
  {
    pattern: /\bchmod\s+-R\s+777/i,
    level: 'medium',
    label: 'Recursive Perm Exposure',
    reason: 'Recursively sets 777 permissions',
  },
  {
    pattern: /\bchown\b/i,
    level: 'medium',
    label: 'Ownership Change',
    reason: 'Changes file ownership',
  },

  // ── MEDIUM: Potentially dangerous eval ──
  {
    pattern: /\beval\s/i,
    level: 'medium',
    label: 'Eval Execution',
    reason: 'Evaluates a string as shell code',
  },

  // ── MEDIUM: Environment / system modification ──
  {
    pattern: /\bsystemctl\s+(stop|disable|mask)/i,
    level: 'medium',
    label: 'Service Modification',
    reason: 'Stops or disables a system service',
  },
  {
    pattern: /\bservice\s+\S+\s+stop/i,
    level: 'medium',
    label: 'Service Stop',
    reason: 'Stops a system service',
  },
  // ── SQL Destructive Operations ───────────────────
  {
    pattern: /\bDELETE\s+FROM\b/i,
    level: 'critical',
    label: 'SQL Delete',
    reason: 'Deletes rows from a database table',
  },
  {
    pattern: /\bDROP\s+(TABLE|DATABASE|SCHEMA|INDEX|VIEW)\b/i,
    level: 'critical',
    label: 'SQL Drop',
    reason: 'Permanently drops a database object',
  },
  {
    pattern: /\bTRUNCATE\s+(TABLE)?\b/i,
    level: 'critical',
    label: 'SQL Truncate',
    reason: 'Removes all rows from a table',
  },
  {
    pattern: /\bALTER\s+TABLE\b.*\bDROP\b/i,
    level: 'high',
    label: 'SQL Alter Drop',
    reason: 'Drops a column or constraint from a table',
  },
  {
    pattern: /\bUPDATE\s+\S+\s+SET\b/i,
    level: 'high',
    label: 'SQL Update',
    reason: 'Modifies existing data in a table',
  },
  {
    pattern: /\bINSERT\s+(INTO|OVERWRITE)\b/i,
    level: 'medium',
    label: 'SQL Insert',
    reason: 'Inserts data into a database table',
  },
];

// ── Classifier function ────────────────────────────────────────────────────

/**
 * Classify risk level of a command or tool invocation.
 * Checks tool name + args against known dangerous patterns.
 */
export function classifyCommandRisk(
  toolName: string,
  args?: Record<string, unknown>,
): RiskClassification | null {
  // Build a searchable string from tool + args
  const searchStr = buildSearchString(toolName, args);
  if (!searchStr) return null;

  for (const dp of DANGER_PATTERNS) {
    if (dp.pattern.test(searchStr)) {
      return {
        level: dp.level,
        label: dp.label,
        reason: dp.reason,
        matchedPattern: dp.pattern.source,
      };
    }
  }
  return null; // no dangerous pattern matched
}

/**
 * Returns true if the command involves privilege escalation (sudo/su/doas/pkexec/runas).
 * Used for the "auto-deny privilege escalation" toggle.
 */
export function isPrivilegeEscalation(toolName: string, args?: Record<string, unknown>): boolean {
  const searchStr = buildSearchString(toolName, args);
  if (!searchStr) return false;
  return /\b(sudo|su\s|doas|pkexec|runas)\b/i.test(searchStr);
}

/**
 * Build a searchable string from tool name + relevant args.
 * Only exec/shell tools have their command checked against danger patterns.
 * Other tools (memory_store, soul_write, fetch, etc.) only check the tool name
 * to avoid false positives from content containing security-related words.
 */
function buildSearchString(toolName: string, args?: Record<string, unknown>): string {
  const parts: string[] = [toolName || ''];
  if (args) {
    // For exec-style tools, include the command argument
    if (toolName === 'exec' || toolName === 'shell' || toolName === 'run') {
      for (const v of Object.values(args)) {
        if (typeof v === 'string') {
          parts.push(v);
        } else if (Array.isArray(v)) {
          parts.push(v.map(String).join(' '));
        } else if (v && typeof v === 'object') {
          parts.push(JSON.stringify(v));
        }
      }
    }
    // For other tools, only include operation-relevant args (url, path, file)
    // but NOT content/body/text which would cause false positives
    else {
      const safeKeys = ['url', 'path', 'file', 'filename', 'destination', 'target'];
      for (const [k, v] of Object.entries(args)) {
        if (safeKeys.includes(k) && typeof v === 'string') {
          parts.push(v);
        }
      }
    }
  }
  return parts.join(' ');
}

// ── Security settings (persisted in encrypted SQLite DB) ───────────────────
// Settings are cached in memory for synchronous access.  On save, the cache
// is updated immediately and flushed to the DB asynchronously.  On startup,
// call initSecuritySettings() to hydrate the cache from the DB.

import {
  loadSecuritySettingsFromDb,
  saveSecuritySettingsToDb,
  resetSecuritySettingsInDb,
} from './db';

const SEC_PREFIX = 'paw_security_';

export interface SecuritySettings {
  autoDenyPrivilegeEscalation: boolean; // Auto-deny sudo/su/doas/pkexec
  autoDenyCritical: boolean; // Auto-deny all critical-risk commands
  requireTypeToCritical: boolean; // Require "ALLOW" to approve critical commands
  commandAllowlist: string[]; // Regex patterns for auto-approved commands
  commandDenylist: string[]; // Regex patterns for auto-denied commands
  sessionOverrideUntil: number | null; // Unix timestamp: auto-approve all until this time
  tokenRotationIntervalDays: number; // Auto-rotation schedule: 0 = disabled
  readOnlyProjects: boolean; // Block agent filesystem write tools in project paths
}

const DEFAULT_SETTINGS: SecuritySettings = {
  autoDenyPrivilegeEscalation: true,
  autoDenyCritical: true,
  requireTypeToCritical: true,
  commandAllowlist: [
    '^git\\b',
    '^npm\\b',
    '^npx\\b',
    '^node\\b',
    '^python3?\\b',
    '^pip3?\\b',
    '^cargo\\b',
    '^rustc\\b',
    '^rustup\\b',
    '^go\\b',
    '^java\\b',
    '^javac\\b',
    '^mvn\\b',
    '^gradle\\b',
    '^ruby\\b',
    '^gem\\b',
    '^bundle\\b',
    '^docker\\b',
    '^docker-compose\\b',
    '^kubectl\\b',
    '^terraform\\b',
    '^ansible\\b',
    '^make\\b',
    '^cmake\\b',
    '^gcc\\b',
    '^g\\+\\+\\b',
    '^clang\\b',
    '^ls\\b',
    '^cat\\b',
    '^echo\\b',
    '^pwd$',
    '^cd\\b',
    '^which\\b',
    '^find\\b',
    '^head\\b',
    '^tail\\b',
    '^wc\\b',
    '^grep\\b',
    '^tree\\b',
    '^mkdir\\b',
    '^touch\\b',
    '^cp\\b',
    '^mv\\b',
    '^chmod\\b',
    '^chown\\b',
    '^ln\\b',
    '^tar\\b',
    '^zip\\b',
    '^unzip\\b',
    '^gzip\\b',
    '^curl\\b',
    '^wget\\b',
    '^ssh\\b',
    '^scp\\b',
    '^rsync\\b',
    '^sed\\b',
    '^awk\\b',
    '^sort\\b',
    '^uniq\\b',
    '^cut\\b',
    '^xargs\\b',
    '^tee\\b',
    '^diff\\b',
    '^patch\\b',
    '^env\\b',
    '^export\\b',
    '^source\\b',
    '^date\\b',
    '^whoami$',
    '^hostname$',
    '^uname\\b',
    '^df\\b',
    '^du\\b',
    '^ps\\b',
    '^top\\b',
    '^htop\\b',
    '^free\\b',
    '^lsof\\b',
    '^netstat\\b',
    '^ss\\b',
    '^ping\\b',
    '^traceroute\\b',
    '^dig\\b',
    '^nslookup\\b',
    '^gh\\b',
    '^jq\\b',
    '^yq\\b',
    '^sqlite3\\b',
    '^psql\\b',
    '^mysql\\b',
    '^redis-cli\\b',
    '^mongosh\\b',
    '^code\\b',
    '^vim\\b',
    '^nano\\b',
    '^apt\\b',
    '^apt-get\\b',
    '^brew\\b',
    '^snap\\b',
    '^pip\\b',
    '^pip3\\b',
    '^yarn\\b',
    '^pnpm\\b',
    '^bun\\b',
    '^deno\\b',
    '^ollama\\b',
    '^ffmpeg\\b',
    '^convert\\b',
    '^magick\\b',
    '^pandoc\\b',
    '^wkhtmlto\\b',
    '^open\\b',
    '^xdg-open\\b',
    '^pbcopy\\b',
    '^pbpaste\\b',
    '^xclip\\b',
    '^xsel\\b',
  ],
  commandDenylist: [],
  sessionOverrideUntil: null,
  tokenRotationIntervalDays: 0,
  readOnlyProjects: false,
};

// In-memory cache — populated at startup by initSecuritySettings()
let _cachedSettings: SecuritySettings | null = null;

/**
 * Initialise the security settings cache from the encrypted database.
 * Call once at app startup after initDb(). Migrates any legacy
 * localStorage data automatically on first run.
 */
export async function initSecuritySettings(): Promise<void> {
  try {
    // Migrate from localStorage → DB on first run
    const legacyRaw = localStorage.getItem(`${SEC_PREFIX}settings`);
    if (legacyRaw) {
      const fromDb = await loadSecuritySettingsFromDb();
      if (!fromDb) {
        // DB is empty — migrate the legacy settings
        await saveSecuritySettingsToDb(legacyRaw);
      }
      // Either way, clear localStorage so XSS can no longer read it
      localStorage.removeItem(`${SEC_PREFIX}settings`);
    }

    const fromDb = await loadSecuritySettingsFromDb();
    if (fromDb) {
      _cachedSettings = { ...DEFAULT_SETTINGS, ...fromDb } as SecuritySettings;
    } else {
      _cachedSettings = { ...DEFAULT_SETTINGS };
    }
  } catch (e) {
    console.warn('[security] Failed to init settings from DB, using defaults:', e);
    _cachedSettings = { ...DEFAULT_SETTINGS };
  }
}

/**
 * Load current security settings (synchronous — reads from in-memory cache).
 * If the cache hasn't been initialised yet (app cold-start), falls back to defaults.
 */
export function loadSecuritySettings(): SecuritySettings {
  if (_cachedSettings) return { ..._cachedSettings };
  return { ...DEFAULT_SETTINGS };
}

/**
 * Save security settings — updates in-memory cache immediately and flushes
 * to the encrypted database asynchronously.
 */
export function saveSecuritySettings(settings: SecuritySettings): void {
  _cachedSettings = { ...settings };
  saveSecuritySettingsToDb(JSON.stringify(settings)).catch((e) =>
    console.warn('[security] Failed to persist settings to DB:', e),
  );
}

/**
 * Reset security settings to defaults — clears DB row and resets cache.
 */
export async function resetSecuritySettings(): Promise<void> {
  _cachedSettings = { ...DEFAULT_SETTINGS };
  await resetSecuritySettingsInDb();
}

/**
 * Extract the command string for allowlist/denylist matching.
 * Only returns the actual command for exec-type tools.
 * For other tools, returns just the tool name to avoid content false positives.
 */
export function extractCommandString(toolName: string, args?: Record<string, unknown>): string {
  if (toolName === 'exec' || toolName === 'shell' || toolName === 'run') {
    return args
      ? Object.values(args)
          .filter((v) => typeof v === 'string')
          .join(' ')
      : toolName;
  }
  return toolName;
}

// ── Safe Regex Helpers (ReDoS mitigation) ─────────────────────────────────

/**
 * Detect regex patterns with nested quantifiers that can cause catastrophic
 * backtracking (ReDoS). Rejects patterns like (a+)+, (a*)+, (a+)*, etc.
 */
const NESTED_QUANTIFIER_RE = /([+*])\s*\)[\s]*[+*?{]/;
const OVERLAPPING_ALTERNATION_RE = /(\.\*.*\|.*\.\*)/;

export function isReDoSRisk(pattern: string): boolean {
  return NESTED_QUANTIFIER_RE.test(pattern) || OVERLAPPING_ALTERNATION_RE.test(pattern);
}

/**
 * Validate a regex pattern string. Returns null if valid, or an error message.
 */
export function validateRegexPattern(pattern: string): string | null {
  if (isReDoSRisk(pattern)) {
    return 'Pattern contains nested quantifiers that could cause catastrophic backtracking';
  }
  try {
    new RegExp(pattern, 'i');
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : 'Invalid regex';
  }
}

/**
 * Safely compile and test a regex pattern against a string.
 * Rejects patterns flagged as ReDoS risks; catches compilation errors.
 */
function safeRegexTest(pattern: string, input: string): boolean {
  if (isReDoSRisk(pattern)) return false;
  try {
    return new RegExp(pattern, 'i').test(input);
  } catch {
    return false;
  }
}

/**
 * Check if a command string matches any pattern in an allowlist.
 * Used for auto-approve of known safe commands.
 */
export function matchesAllowlist(command: string, patterns: string[]): boolean {
  return patterns.some((p) => safeRegexTest(p, command));
}

/**
 * Check if a command string matches any pattern in a denylist.
 * Used for auto-deny of known dangerous commands.
 */
export function matchesDenylist(command: string, patterns: string[]): boolean {
  return patterns.some((p) => safeRegexTest(p, command));
}

// ── Network Request Auditing (Sprint C5) ──────────────────────────────────

/** Tools/commands that perform outbound network requests. */
const NETWORK_TOOLS =
  /\b(curl|wget|fetch|http|nc|ncat|netcat|nmap|ssh|scp|rsync|ftp|sftp|telnet|socat|lynx|aria2c|axel)\b/i;

/** Patterns that suggest data exfiltration (piping data TO a remote host). */
const EXFILTRATION_PATTERNS = [
  /\bcat\b.*\|\s*(curl|wget|nc|ncat)/i, // cat secret | curl
  /\bcurl\b.*-d\s+@/i, // curl -d @file (upload file)
  /\bcurl\b.*--data-binary\s+@/i, // curl --data-binary @file
  /\bcurl\b.*-T\s+/i, // curl -T (upload)
  /\bcurl\b.*--upload-file/i, // curl --upload-file
  /\bwget\b.*--post-file/i, // wget --post-file
  /\bnc\b.*<\s*\//i, // nc host < /file
  /\bscp\b.*:\s*$/i, // scp file host: (outbound)
  /\brsync\b.*[^@]+@[^:]+:/i, // rsync to remote
  />\s*\/dev\/tcp\//i, // bash /dev/tcp redirect
];

/** Known-safe localhost / loopback destinations. */
const SAFE_HOSTS = /\b(localhost|127\.0\.0\.1|0\.0\.0\.0|\[?::1\]?)\b/i;

/** Extract all URLs and hostnames from a command string. */
export function extractNetworkTargets(command: string): string[] {
  const targets: string[] = [];
  // Match full URLs
  const urlRe = /https?:\/\/[^\s'"]+/gi;
  let m: RegExpExecArray | null;
  while ((m = urlRe.exec(command)) !== null) targets.push(m[0]);
  // Match host:port patterns (e.g. nc example.com 443)
  const hostPortRe =
    /(?:^|\s)(?:nc|ncat|netcat|ssh|scp|telnet|socat|ftp|sftp)\s+([a-z0-9._-]+)\s+(\d+)/gi;
  while ((m = hostPortRe.exec(command)) !== null) targets.push(`${m[1]}:${m[2]}`);
  return targets;
}

export interface NetworkAuditResult {
  isNetworkRequest: boolean;
  targets: string[];
  isExfiltration: boolean;
  exfiltrationReason: string | null;
  allTargetsLocal: boolean;
}

/**
 * Analyse a tool invocation for outbound network activity.
 * Returns audit metadata for logging regardless of allow/deny decision.
 */
export function auditNetworkRequest(
  toolName: string,
  args?: Record<string, unknown>,
): NetworkAuditResult {
  const searchStr = buildSearchString(toolName, args);

  const result: NetworkAuditResult = {
    isNetworkRequest: false,
    targets: [],
    isExfiltration: false,
    exfiltrationReason: null,
    allTargetsLocal: true,
  };

  if (!NETWORK_TOOLS.test(searchStr)) return result;

  result.isNetworkRequest = true;
  result.targets = extractNetworkTargets(searchStr);

  // Check if any target is non-local
  result.allTargetsLocal =
    result.targets.length === 0 || result.targets.every((t) => SAFE_HOSTS.test(t));

  // Check for exfiltration patterns
  for (const ep of EXFILTRATION_PATTERNS) {
    if (ep.test(searchStr)) {
      result.isExfiltration = true;
      result.exfiltrationReason = ep.source;
      break;
    }
  }

  return result;
}

// ── Session override helpers ───────────────────────────────────────────────

/** Activate "allow all" override for a given duration in minutes. */
export function activateSessionOverride(durationMinutes: number): void {
  const settings = loadSecuritySettings();
  settings.sessionOverrideUntil = Date.now() + durationMinutes * 60 * 1000;
  saveSecuritySettings(settings);
}

/** Clear the session override. */
export function clearSessionOverride(): void {
  const settings = loadSecuritySettings();
  settings.sessionOverrideUntil = null;
  saveSecuritySettings(settings);
}

/** Check if session override is currently active. Returns remaining ms or 0. */
export function getSessionOverrideRemaining(): number {
  const settings = loadSecuritySettings();
  if (!settings.sessionOverrideUntil) return 0;
  const remaining = settings.sessionOverrideUntil - Date.now();
  if (remaining <= 0) {
    // Expired — auto-clear
    settings.sessionOverrideUntil = null;
    saveSecuritySettings(settings);
    return 0;
  }
  return remaining;
}

// ── Filesystem write tool detection (H3) ───────────────────────────────────

const WRITE_TOOLS =
  /\b(write_file|append_file|delete_file|create_file|mv|cp|rename|remove|delete|mkdir|rmdir|chmod|chown|truncate|append|patch|edit)\b/i;
const WRITE_COMMANDS = /\b(mv|cp|rm|mkdir|rmdir|touch|chmod|chown|truncate|tee|sed\s+-i|install)\b/;

export function isFilesystemWriteTool(
  tool: string,
  args?: Record<string, unknown>,
): { isWrite: boolean; targetPath: string | null } {
  const result = { isWrite: false, targetPath: null as string | null };

  if (WRITE_TOOLS.test(tool)) {
    result.isWrite = true;
    // Try to extract target path from args
    if (args) {
      const pathKeys = ['path', 'filePath', 'file', 'destination', 'dest', 'target', 'directory'];
      for (const key of pathKeys) {
        if (typeof args[key] === 'string') {
          result.targetPath = args[key] as string;
          break;
        }
      }
    }
    return result;
  }

  // Check command string for write commands
  const cmdStr = buildSearchString(tool, args);
  if (WRITE_COMMANDS.test(cmdStr)) {
    result.isWrite = true;
  }

  return result;
}
