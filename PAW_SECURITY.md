# Paw — Security Architecture & Roadmap

Paw is a Tauri 2 desktop wrapper around OpenClaw. Because every system call flows through Paw's Rust backend before reaching the OS, it's the natural enforcement point for security controls. This document tracks what's built, what's missing, and what to build next.

---

## Table of Contents

1. [Architecture Advantage](#architecture-advantage)
2. [What's Already Built](#whats-already-built)
3. [What's Missing — Critical](#whats-missing--critical)
4. [What's Missing — High Priority](#whats-missing--high-priority)
5. [What's Missing — Medium Priority](#whats-missing--medium-priority)
6. [Implementation Plan](#implementation-plan)
7. [File Reference](#file-reference)

---

## Architecture Advantage

```
┌─────────────────────────────────────────────────────────┐
│  User (Paw UI)                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Frontend (TypeScript)                           │   │
│  │  • Approval modal (Allow/Deny)                   │   │
│  │  • Permission toggles                            │   │
│  │  • Audit log viewer                              │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │ Tauri IPC (structured commands)        │
│  ┌──────────────▼───────────────────────────────────┐   │
│  │  Rust Backend (lib.rs)                           │   │
│  │  • OS Keychain (keyring crate)                   │   │
│  │  • File permissions (chmod 0o600)                │   │
│  │  • Filesystem scope enforcement                  │   │
│  │  • ⚠️  NO command filtering yet                  │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │                                        │
│  ┌──────────────▼───────────────────────────────────┐   │
│  │  OpenClaw Gateway (WebSocket)                    │   │
│  │  • exec.approval.requested → Paw approves/denies│   │
│  │  • Agent tool calls intercepted                   │   │
│  │  • Auth token required for connection             │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │                                        │
│                 ▼                                        │
│           Operating System                               │
└─────────────────────────────────────────────────────────┘
```

**Key insight**: The agent never touches the OS directly. Every tool call goes through `exec.approval.requested` → Paw shows a modal → user decides. This is the Human-in-the-Loop (HIL) protocol. The gap is that Paw currently shows all requests the same way — `sudo rm -rf /` looks identical to `git status`.

---

## What's Already Built

### 1. HIL — Human-in-the-Loop (Exec Approvals) ✅

| Component | Status | Location |
|-----------|--------|----------|
| Live approval modal | ✅ | `main.ts:3021` — `exec.approval.requested` event listener |
| Allow / Deny buttons | ✅ | `main.ts:3081-3089` — resolves via `exec.approvals.resolve` |
| Approval policy toggles | ✅ | Settings → visual toggle switches per category |
| Allowlist / Denylist config | ✅ | `exec.approvals.get/set` — per-tool-category allow/deny |
| Per-node approval rules | ✅ | `exec.approvals.node.get/set` — separate rules for remote nodes |
| Auto-deny for mail permissions | ✅ | `classifyMailPermission()` checks Credential Vault toggles first |
| Approval details shown | ✅ | Modal shows tool name + full JSON args |

**Gap**: No risk classification. A `sudo` command looks the same as `ls`. No command allowlisting.

### 2. File Permissions (chmod) ✅

| Component | Status | Location |
|-----------|--------|----------|
| `set_owner_only_permissions()` | ✅ | `lib.rs:12` — sets `0o600` on sensitive files |
| Applied to himalaya config | ✅ | `lib.rs:1646` — after writing email config |
| Applied after account removal | ✅ | `lib.rs:1753` — after removing account from config |
| No-op on non-Unix | ✅ | `lib.rs:20` — Windows ACL not handled |

**Gap**: Only protects Paw's own config files. Agent can run `chmod 777` on anything else.

### 3. OS Keychain ✅

| Component | Status | Location |
|-----------|--------|----------|
| Password storage | ✅ | `lib.rs:1570` — `keyring::Entry::new()` + `.set_password()` |
| Keychain check | ✅ | `keyring_has_password` Tauri command |
| Keychain delete | ✅ | `keyring_delete_password` Tauri command |
| Never plaintext | ✅ | Config files contain keyring references, not passwords |

**No gaps** — this is solid.

### 4. Tauri Filesystem Scope ✅ (Partial)

| Component | Status | Location |
|-----------|--------|----------|
| Scoped to `~/Documents/Paw/**` | ✅ | `capabilities/default.json:23-27` |
| Shell limited to `open` | ✅ | `capabilities/default.json:29` — `shell:allow-open` only |
| SQL operations scoped | ✅ | `sql:allow-load/execute/select/close` |

**Gap**: Scope is broad — entire `~/Documents/Paw` tree. No per-project scoping. Projects view uses `window.__TAURI__?.dialog` which bypasses scope.

### 5. Gateway Auth Token ✅

| Component | Status | Location |
|-----------|--------|----------|
| Token-based WebSocket auth | ✅ | `gateway.ts` — sends token on connect |
| Token read from config | ✅ | `lib.rs` — `get_gateway_token` reads `~/.openclaw/openclaw.json` |
| Per-device token rotation | ✅ | `device.token.rotate` wired in Settings |
| Per-device token revocation | ✅ | `device.token.revoke` wired in Settings |

**Gap**: No auto-rotation schedule. Token never expires unless manually rotated.

### 6. Credential Audit Log ✅

| Component | Status | Location |
|-----------|--------|----------|
| SQLite `credential_activity_log` table | ✅ | `db.ts:139` |
| Logs all agent email actions | ✅ | `db.ts:338` — insert with action, tool, detail, was_allowed |
| Viewable in Mail sidebar | ✅ | Mail view → collapsible activity log |

**Gap**: Only covers email/credential actions. No unified audit trail for shell commands, file writes, network requests.

### 7. Channel Access Policies ✅

| Component | Status | Location |
|-----------|--------|----------|
| DM Policy per channel | ✅ | `main.ts:1676` — pairing / allowlist / open / disabled |
| Group Policy per channel | ✅ | `main.ts:1686` — allowlist / open / disabled |
| Allowed users list | ✅ | Per-channel comma-separated allowlist |

**No gaps** for channel-level access control.

### 8. Config Redaction Warning ✅

| Component | Status | Location |
|-----------|--------|----------|
| Detects `__OPENCLAW_REDACTED__` | ✅ | `main.ts` — warns before saving corrupted config |
| Confirmation dialog | ✅ | User must explicitly confirm to save redacted values |

---

## What's Missing — Critical

### C1. sudo / su / Privilege Escalation Detection ✅

**Risk**: Agent can request `sudo rm -rf /`, `su -c 'dangerous'`, `doas`, `pkexec`, or any privilege escalation command. The approval modal shows it, but it looks the same as a safe command. Users click "Allow" without reading.

**Built** (Sprint A — 2026-02-15):

- [x] **Dangerous command classifier** — `src/security.ts` pattern-matches exec approval requests against 30+ danger patterns across critical/high/medium risk levels
- [x] **Red "DANGER" modal variant** — when a dangerous command is detected, shows a visually distinct warning (red border, skull icon, risk banner with level + reason)
- [x] **Auto-deny option for privilege escalation** — toggle in Settings: "Auto-deny privilege escalation" blocks sudo/su/doas/pkexec/runas automatically
- [x] **Auto-deny all critical commands** — toggle in Settings: "Auto-deny all critical-risk commands"
- [x] **Require explicit typing to approve** — for critical commands, user must type "ALLOW" instead of clicking a button (configurable)

**Patterns to detect**:
```
sudo, su, doas, pkexec, runas           — privilege escalation
rm -rf /, rm -rf ~, rm -rf /*          — destructive deletion
chmod 777, chmod -R 777                 — permission exposure
dd if=, mkfs, fdisk                     — disk destruction
:(){ :|:& };:                           — fork bomb
> /dev/sda, > /dev/null (pipes)         — device writes
curl | sh, wget | sh, curl | bash      — remote code execution
eval, exec (with untrusted input)       — code injection
kill -9 1, killall                      — process termination
iptables -F, ufw disable               — firewall disabling
passwd, chpasswd, usermod               — user account modification
crontab -r                              — cron destruction
ssh-keygen -f (overwriting)             — key destruction
```

**Location**: Enhance `exec.approval.requested` handler in `main.ts:3021`

### C2. Content Security Policy (CSP) ✅

**Built** (Sprint A — 2026-02-15):

- [x] **Set restrictive CSP** in `tauri.conf.json` — `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' ws://127.0.0.1:* ...; img-src 'self' data: blob:; object-src 'none'; frame-ancestors 'none'`
- [x] **Block external script loading** — only `'self'` origin allowed
- [x] **Restrict WebSocket connections** — only localhost origins
- [x] **Allow data: URIs for images** — needed for attachment previews

### C3. Command Allowlist / Denylist ✅

**Built** (Sprint A — 2026-02-15):

- [x] **Command allowlist in Settings** — regex patterns for auto-approved commands with sensible defaults (git, npm, node, python, ls, cat, etc.)
- [x] **Command denylist in Settings** — regex patterns for auto-denied commands with sensible defaults (sudo, rm -rf /, chmod 777, curl|sh, etc.)
- [x] **Policy modes** — auto-deny (privilege escalation, critical), auto-approve (allowlist), or manual (modal) per command
- [x] **Persist to localStorage** — `paw_security_settings` key with all toggles + patterns
- [x] **New SQLite tables** — `security_audit_log` for unified event logging, `security_rules` for persistent rule storage
- [x] **Regex validation** — patterns are validated before saving, invalid regex shows error toast

**Location**: `src/security.ts`, Settings view (`settings.ts`), `db.ts` (new tables)

**Still TODO**:
- [ ] **Per-session overrides** — "Allow all for this session" with a timer (30min, 1hr)

---

## What's Missing — High Priority

### H1. Unified Security Audit Dashboard ✅

**Built** (Sprint A + Sprint B — 2026-02-15):

- [x] **New SQLite table `security_audit_log`** — all security-relevant events with: event_type, risk_level, tool_name, command, detail, session_key, was_allowed, matched_pattern
- [x] **`security_rules` table** — persistent storage for user-defined allow/deny patterns
- [x] **Logging from exec approval handler** — all auto-deny, auto-allow, and user decisions are logged to `security_audit_log`
- [x] **CRUD functions** — `logSecurityEvent()`, `getSecurityAuditLog()`, `listSecurityRules()`, `addSecurityRule()`, etc.
- [x] **Audit dashboard UI** — dedicated section in Settings with sortable/filterable table
- [x] **Filterable by type, date, severity** — dropdown filters for event type, risk level, and result count
- [x] **Export to JSON/CSV** — one-click export buttons in the dashboard
- [x] **Security score widget** — quick stats: CSP active, blocked count, allowed count, critical count

### H2. Skill Vetting / Package Safety 🔶 PARTIAL

**Risk**: `skills.install` installs npm packages with no safety check. A malicious skill package could contain arbitrary code.

**Built** (Sprint B — 2026-02-15):

- [x] **Pre-install safety confirmation** — modal dialog showing safety checks before every skill install
- [x] **Known-safe skills list** — built-in set of community-vetted skill names (web-search, git, memory, etc.)
- [x] **Warning for unrecognized packages** — "Unrecognized skill — not in known-safe list" with ⚠ badge
- [x] **npm install script warning** — alerts that npm packages run install scripts
- [x] **Security audit logging** — all skill install decisions logged to `security_audit_log`

**Still TODO**:

- [ ] **npm audit integration** — actually run `npm audit` on the package before install
- [ ] **Risk score display** — show download count, last publish date, known vulnerabilities
- [ ] **Post-install sandbox check** — verify the skill doesn't request unexpected permissions

### H3. Filesystem Sandboxing Hardening 🔶 PARTIAL

**Current**: Tauri scope is `$HOME/Documents/Paw/**`. Projects view can browse any directory.

**Built** (Sprint B — 2026-02-15):

- [x] **Sensitive path blocking** — blocks adding `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.kube`, `~/.docker`, `~/.openclaw`, `/etc`, `/root`, `/proc`, `/sys`, `/dev`, filesystem root, and home directory root as project folders
- [x] **File tree protection** — sensitive directories hidden from file tree browsing within projects
- [x] **Security audit logging** — blocked path attempts logged to `security_audit_log`

**Still TODO**:

- [ ] **Per-project filesystem scope** — when a project folder is added, scope Tauri to that specific path
- [ ] **Read-only mode** — option to browse project files read-only
- [ ] **Agent filesystem restrictions** — through exec approvals, restrict which paths the agent can write to

### H4. Token Auto-Rotation 🔶 PARTIAL

**Current**: Gateway token never expires unless manually rotated via `device.token.rotate`.

**Built** (Sprint B — 2026-02-15):

- [x] **Token age display** — device cards show days since pairing
- [x] **Rotation reminder** — stale (30d+) and critical (90d+) visual warnings on device cards

**Still TODO**:

- [ ] **Auto-rotation schedule** — configurable interval (weekly, monthly, on-upgrade)
- [ ] **Auto-rotate on update** — when OpenClaw is updated, offer to rotate the token

---

## What's Missing — Medium Priority

### M1. Network Sandboxing ❌

**Risk**: The agent can make HTTP requests to any domain via tool calls.

**What to build**:

- [ ] **Outbound domain allowlist** — restrict which domains the agent can reach
- [ ] **Log all outbound requests** — audit trail of network activity
- [ ] **Block exfiltration patterns** — detect if agent is sending local data to unknown domains

**Difficulty**: Hard — requires either a proxy layer or Tauri plugin. May need to inspect exec approval args for `curl`/`wget`/`fetch` commands.

### M2. Encryption at Rest ❌

**Risk**: `paw.db` (SQLite) stores email data, credential logs, project files in plaintext on disk.

**What to build**:

- [ ] **SQLCipher integration** — encrypt the SQLite database
- [ ] **Key derivation from OS keychain** — derive encryption key from a keychain-stored secret
- [ ] **Encrypt config files** — `~/.openclaw/openclaw.json` contains the gateway auth token in plaintext

### M3. Session Isolation ❌

**Risk**: All sessions share the same agent context. One session could theoretically access another's data.

**What to build**:

- [ ] **Session-aware exec approvals** — show which session is requesting a tool call
- [ ] **Per-session filesystem scope** — different sessions get different allowed paths
- [ ] **Session kill switch** — terminate a specific session's agent run if it goes rogue

### M4. Gateway Connection Security ❌

**Current**: WebSocket connects to `ws://127.0.0.1:port` — unencrypted localhost.

**What to build**:

- [ ] **Validate localhost-only** — ensure gateway never binds to `0.0.0.0`
- [ ] **Optional TLS for remote gateway** — support `wss://` when gateway is running on a different machine
- [ ] **Connection integrity check** — verify the handshake nonce to prevent MITM on localhost

### M5. Crash Recovery & Watchdog ❌

**Risk**: If the gateway crashes, the agent dies silently. No auto-restart, no user guidance.

**What to build**:

- [ ] **Health check watchdog** — periodic ping, auto-restart gateway if it dies
- [ ] **Crash notification** — "Gateway stopped unexpectedly. Restart now?"
- [ ] **Crash log capture** — save last stdout/stderr before crash

---

## Implementation Plan

### Sprint A — Critical Security ✅ COMPLETE (2026-02-15)

| # | Task | Status | Files |
|---|------|--------|-------|
| A1 | Dangerous command classifier + red modal | ✅ | `src/security.ts` (new), `main.ts`, `styles.css`, `index.html` |
| A2 | Set proper CSP in tauri.conf.json | ✅ | `src-tauri/tauri.conf.json` |
| A3 | Command allowlist/denylist UI in Settings | ✅ | `settings.ts`, `db.ts` (2 new tables), `main.ts`, `index.html` |
| A4 | Auto-deny sudo/su toggle | ✅ | `src/security.ts`, `main.ts`, `settings.ts`, `index.html` |

### Sprint B — Audit & Trust ✅ COMPLETE (2026-02-15)

| # | Task | Status | Files |
|---|------|--------|-------|
| B1 | Security audit dashboard UI | ✅ | `settings.ts`, `index.html`, `styles.css` |
| B2 | Skill vetting pre-install safety | ✅ | `skills.ts` (safety confirmation dialog + known-safe list) |
| B3 | Token age display + rotation reminder | ✅ | `settings.ts` (device cards with age + stale/critical warnings) |
| B4 | Sensitive path blocking for Projects | ✅ | `projects.ts` (20+ blocked paths + file tree filtering) |

### Sprint C — Hardening (Later)

| # | Task | Effort | Files |
|---|------|--------|-------|
| C1 | Per-project Tauri scope | 1 day | `capabilities/default.json`, `projects.ts` |
| C2 | SQLCipher for paw.db | 2 days | `Cargo.toml`, `lib.rs`, `db.ts` |
| C3 | Gateway localhost validation | 2 hrs | `main.ts` (connect), `lib.rs` (start_gateway) |
| C4 | Crash watchdog + auto-restart | 1 day | `main.ts` (health polling), `lib.rs` |
| C5 | Network request auditing | 1 day | `main.ts` (exec approval args inspection) |

---

## Security Protocols Summary

| Protocol | Status | Paw Component |
|----------|--------|---------------|
| **HIL (Human-in-the-Loop)** | ✅ Built | Exec approval modal, allow/deny per request, mail permission auto-deny |
| **chmod (file permissions)** | ✅ Built (own files) | `set_owner_only_permissions()` on himalaya config — agent `chmod` not blocked |
| **sudo/su detection** | ✅ Built | `src/security.ts` — pattern matching + auto-deny toggle + 30+ danger patterns |
| **Dangerous command classifier** | ✅ Built | Risk classification (critical/high/medium), red DANGER modal, type-to-confirm |
| **Command allowlist/denylist** | ✅ Built | Regex patterns in Settings, auto-approve safe commands, auto-deny dangerous ones |
| **OS Keychain** | ✅ Built | Rust `keyring` crate — macOS Keychain / libsecret / Windows |
| **Filesystem sandbox** | ✅ Partial | Tauri scope `~/Documents/Paw/**` — Projects view is unscoped |
| **CSP** | ✅ Built | Restrictive CSP: self-only scripts, localhost-only WebSocket, no external loads |
| **Gateway auth** | ✅ Built | Token-based WebSocket auth + per-device rotation/revocation |
| **Credential audit** | ✅ Built (mail only) | `credential_activity_log` table — no unified audit dashboard |
| **Security audit log** | ✅ Built | `security_audit_log` table + filterable dashboard UI with export (JSON/CSV) |
| **Channel access control** | ✅ Built | Per-channel DM/group policies with allowlists |
| **Skill vetting** | 🔶 Partial | Pre-install safety confirmation, known-safe list, audit logging — no npm audit yet |
| **Filesystem sandbox** | 🔶 Partial | Tauri scope + sensitive path blocking (20+ patterns) + file tree filtering |
| **Token rotation** | 🔶 Partial | Token age display + stale/critical warnings on device cards — no auto-rotation yet |
| **Encryption at rest** | ❌ Not built | SQLite and config files are plaintext |
| **Token auto-rotation** | 🔶 Partial | Token age display + rotation reminders — no auto-schedule yet |
| **Network sandboxing** | ❌ Not built | Agent can reach any domain |
| **Crash recovery** | ❌ Not built | No watchdog, no auto-restart |

---

## File Reference

| File | Security Role |
|------|---------------|
| `src-tauri/src/lib.rs` | Rust backend — keychain, chmod, gateway lifecycle |
| `src-tauri/tauri.conf.json` | CSP config (restrictive policy set), bundle config |
| `src-tauri/capabilities/default.json` | Filesystem scope, shell permissions |
| `src/security.ts` | **NEW** — Dangerous command classifier, risk patterns, security settings load/save, allowlist/denylist matching |
| `src/main.ts:3022-3210` | Exec approval handler — risk classification, auto-deny/allow, danger modal, type-to-confirm |
| `src/main.ts:1660-1820` | Channel setup — DM/group policies, allowlists |
| `src/gateway.ts:607-625` | Exec approval gateway methods |
| `src/db.ts:139` | `credential_activity_log` table schema |
| `src/db.ts:150-190` | **NEW** — `security_audit_log` + `security_rules` table schemas |
| `src/db.ts:380-460` | **NEW** — Security audit log + rules CRUD functions |
| `src/views/settings.ts` | Approval toggles, security policy toggles, audit dashboard, device token management |
| `src/views/skills.ts` | Skill install (with safety confirmation + known-safe list) |
| `src/views/projects.ts` | File browser (sensitive path blocking for 20+ patterns) |

---

## OpenClaw Security Context

Paw wraps OpenClaw, which has its own security:
- **Gateway auth token** — required for all WebSocket communication
- **Exec approval system** — agent tool calls require approval before executing
- **Channel pairing** — new contacts must be approved before interacting
- **Device pairing** — new devices must be approved before connecting

What OpenClaw does NOT have:
- No command classification (safe vs dangerous)
- No command allowlist/denylist
- No filesystem sandboxing (it runs as the user's process)
- No network restrictions
- No encryption at rest

**This is exactly why Paw is the right place to add these controls** — it sits between the user and OpenClaw, with Rust-level enforcement capabilities.
