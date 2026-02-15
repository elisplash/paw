// Paw — Security: Dangerous Command Classifier & Policy Engine
// Classifies exec approval requests by risk level and enforces command policies.

// ── Risk levels ────────────────────────────────────────────────────────────

export type RiskLevel = 'critical' | 'high' | 'medium' | 'low' | 'safe';

export interface RiskClassification {
  level: RiskLevel;
  label: string;
  reason: string;         // human-readable explanation
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
  { pattern: /\bsudo\b/i,          level: 'critical', label: 'Privilege Escalation',    reason: 'Uses sudo to run commands as root' },
  { pattern: /\bsu\s+(-|root|\w)/i,level: 'critical', label: 'Privilege Escalation',    reason: 'Switches to another user (su)' },
  { pattern: /\bdoas\b/i,          level: 'critical', label: 'Privilege Escalation',    reason: 'Uses doas to run commands as root' },
  { pattern: /\bpkexec\b/i,        level: 'critical', label: 'Privilege Escalation',    reason: 'Uses pkexec for privilege escalation' },
  { pattern: /\brunas\b/i,         level: 'critical', label: 'Privilege Escalation',    reason: 'Uses runas to run as another user' },

  // ── CRITICAL: Destructive deletion ──
  { pattern: /\brm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*r[a-zA-Z]*\s+)?|(-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*f[a-zA-Z]*\s+)?))[\/"'~*]/i,
                                    level: 'critical', label: 'Destructive Deletion',    reason: 'Recursive forced deletion targeting root, home, or wildcard paths' },
  { pattern: /\brm\s+-rf\s*\//i,   level: 'critical', label: 'Destructive Deletion',    reason: 'rm -rf / — destroys the entire filesystem' },
  { pattern: /\brm\s+-rf\s+~/i,    level: 'critical', label: 'Destructive Deletion',    reason: 'rm -rf ~ — destroys the home directory' },

  // ── CRITICAL: Disk destruction ──
  { pattern: /\bdd\s+if=/i,        level: 'critical', label: 'Disk Write',              reason: 'dd can overwrite disk partitions or devices' },
  { pattern: /\bmkfs\b/i,          level: 'critical', label: 'Disk Format',             reason: 'mkfs formats a disk partition' },
  { pattern: /\bfdisk\b/i,         level: 'critical', label: 'Disk Partition',           reason: 'fdisk modifies disk partitions' },
  { pattern: />\s*\/dev\/sd/i,     level: 'critical', label: 'Device Write',            reason: 'Writing directly to a block device' },

  // ── CRITICAL: Fork bomb ──
  { pattern: /:\(\)\s*\{.*\|.*&\s*\}\s*;?\s*:/,
                                    level: 'critical', label: 'Fork Bomb',               reason: 'Shell fork bomb — will crash the system' },

  // ── CRITICAL: Remote code execution ──
  { pattern: /\bcurl\b.*\|\s*(ba)?sh/i,  level: 'critical', label: 'Remote Code Exec', reason: 'Downloads and executes remote script (curl | sh)' },
  { pattern: /\bwget\b.*\|\s*(ba)?sh/i,  level: 'critical', label: 'Remote Code Exec', reason: 'Downloads and executes remote script (wget | sh)' },
  { pattern: /\bcurl\b.*\|\s*python/i,   level: 'critical', label: 'Remote Code Exec', reason: 'Downloads and pipes to python interpreter' },
  { pattern: /\bwget\b.*\|\s*python/i,   level: 'critical', label: 'Remote Code Exec', reason: 'Downloads and pipes to python interpreter' },

  // ── HIGH: Firewall / network security ──
  { pattern: /\biptables\s+-F/i,   level: 'high', label: 'Firewall Flush',              reason: 'Flushes all iptables/firewall rules' },
  { pattern: /\bufw\s+disable/i,   level: 'high', label: 'Firewall Disable',            reason: 'Disables the UFW firewall' },
  { pattern: /\bfirewalld?\b.*stop/i, level: 'high', label: 'Firewall Stop',            reason: 'Stops the firewall daemon' },

  // ── HIGH: User/account modification ──
  { pattern: /\bpasswd\b/i,        level: 'high', label: 'Password Change',             reason: 'Modifies user passwords' },
  { pattern: /\bchpasswd\b/i,      level: 'high', label: 'Password Change',             reason: 'Batch modifies user passwords' },
  { pattern: /\busermod\b/i,       level: 'high', label: 'User Modification',           reason: 'Modifies user account properties' },
  { pattern: /\buseradd\b/i,       level: 'high', label: 'User Creation',               reason: 'Creates a new user account' },
  { pattern: /\buserdel\b/i,       level: 'high', label: 'User Deletion',               reason: 'Deletes a user account' },

  // ── HIGH: Process killing ──
  { pattern: /\bkill\s+-9\s+1\b/i, level: 'high', label: 'Kill Init',                  reason: 'Sends SIGKILL to PID 1 (init)' },
  { pattern: /\bkillall\b/i,       level: 'high', label: 'Kill All Processes',          reason: 'Kills all processes matching a name' },

  // ── HIGH: Cron / scheduled task destruction ──
  { pattern: /\bcrontab\s+-r\b/i,  level: 'high', label: 'Cron Wipe',                   reason: 'Removes all crontab entries' },

  // ── HIGH: SSH key destruction ──
  { pattern: /\bssh-keygen\b.*-f/i, level: 'high', label: 'SSH Key Overwrite',          reason: 'May overwrite existing SSH keys' },

  // ── MEDIUM: Permission changes ──
  { pattern: /\bchmod\s+(777|a\+rwx)/i,  level: 'medium', label: 'Permission Exposure', reason: 'Sets world-readable/writable permissions (777)' },
  { pattern: /\bchmod\s+-R\s+777/i,      level: 'medium', label: 'Recursive Perm Exposure', reason: 'Recursively sets 777 permissions' },
  { pattern: /\bchown\b/i,                level: 'medium', label: 'Ownership Change',    reason: 'Changes file ownership' },

  // ── MEDIUM: Potentially dangerous eval ──
  { pattern: /\beval\s/i,          level: 'medium', label: 'Eval Execution',             reason: 'Evaluates a string as shell code' },

  // ── MEDIUM: Environment / system modification ──
  { pattern: /\bsystemctl\s+(stop|disable|mask)/i,
                                    level: 'medium', label: 'Service Modification',       reason: 'Stops or disables a system service' },
  { pattern: /\bservice\s+\S+\s+stop/i,
                                    level: 'medium', label: 'Service Stop',               reason: 'Stops a system service' },
];

// ── Classifier function ────────────────────────────────────────────────────

/**
 * Classify risk level of a command or tool invocation.
 * Checks tool name + args against known dangerous patterns.
 */
export function classifyCommandRisk(
  toolName: string,
  args?: Record<string, unknown>
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
 * Build a searchable string from tool name + all arg values (flattened).
 */
function buildSearchString(toolName: string, args?: Record<string, unknown>): string {
  const parts: string[] = [toolName || ''];
  if (args) {
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
  return parts.join(' ');
}

// ── Security settings (persisted in localStorage) ──────────────────────────

const SEC_PREFIX = 'paw_security_';

export interface SecuritySettings {
  autoDenyPrivilegeEscalation: boolean;  // Auto-deny sudo/su/doas/pkexec
  autoDenyCritical: boolean;             // Auto-deny all critical-risk commands
  requireTypeToCritical: boolean;        // Require "ALLOW" to approve critical commands
  commandAllowlist: string[];            // Regex patterns for auto-approved commands
  commandDenylist: string[];             // Regex patterns for auto-denied commands
}

const DEFAULT_SETTINGS: SecuritySettings = {
  autoDenyPrivilegeEscalation: false,
  autoDenyCritical: false,
  requireTypeToCritical: true,
  commandAllowlist: [
    '^git\\b',
    '^npm\\b',
    '^npx\\b',
    '^node\\b',
    '^python3?\\b',
    '^pip3?\\b',
    '^ls\\b',
    '^cat\\b',
    '^echo\\b',
    '^pwd$',
    '^which\\b',
    '^find\\b',
    '^head\\b',
    '^tail\\b',
    '^wc\\b',
    '^grep\\b',
    '^tree\\b',
  ],
  commandDenylist: [
    '\\bsudo\\b',
    '\\bsu\\s',
    '\\brm\\s+-rf\\s+/',
    '\\bchmod\\s+777',
    '\\bdd\\s+if=',
    '\\bcurl\\b.*\\|\\s*(ba)?sh',
    '\\bwget\\b.*\\|\\s*(ba)?sh',
  ],
};

export function loadSecuritySettings(): SecuritySettings {
  try {
    const raw = localStorage.getItem(`${SEC_PREFIX}settings`);
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { ...DEFAULT_SETTINGS };
}

export function saveSecuritySettings(settings: SecuritySettings): void {
  localStorage.setItem(`${SEC_PREFIX}settings`, JSON.stringify(settings));
}

/**
 * Check if a command string matches any pattern in an allowlist.
 * Used for auto-approve of known safe commands.
 */
export function matchesAllowlist(command: string, patterns: string[]): boolean {
  return patterns.some(p => {
    try { return new RegExp(p, 'i').test(command); }
    catch { return false; }
  });
}

/**
 * Check if a command string matches any pattern in a denylist.
 * Used for auto-deny of known dangerous commands.
 */
export function matchesDenylist(command: string, patterns: string[]): boolean {
  return patterns.some(p => {
    try { return new RegExp(p, 'i').test(command); }
    catch { return false; }
  });
}
