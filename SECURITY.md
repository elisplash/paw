# Security

Pawz is a Tauri v2 desktop AI agent. Every system call flows through the Rust backend before reaching the OS, making it the natural enforcement point for all security controls.

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  User (Pawz UI)                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Frontend (TypeScript)                             │  │
│  │  • Approval modal (Allow / Deny / type "ALLOW")    │  │
│  │  • Security policy toggles                         │  │
│  │  • Audit dashboard with export                     │  │
│  └──────────────┬─────────────────────────────────────┘  │
│                 │ Tauri IPC (structured commands)         │
│  ┌──────────────▼─────────────────────────────────────┐  │
│  │  Rust Engine Backend                               │  │
│  │  • Tool executor with HIL approval flow            │  │
│  │  • Command risk classifier                         │  │
│  │  • Prompt injection scanner                        │  │
│  │  • OS keychain (keyring crate)                     │  │
│  │  • AES-256-GCM field encryption                    │  │
│  │  • Filesystem scope enforcement                    │  │
│  │  • Container sandbox (Docker via bollard)           │  │
│  │  • Channel access control (pairing + allowlists)   │  │
│  └──────────────┬─────────────────────────────────────┘  │
│                 │                                        │
│                 ▼                                        │
│           Operating System                               │
└──────────────────────────────────────────────────────────┘
```

**Key design principle**: The agent never touches the OS directly. Every tool call goes through the Rust tool executor. Read-only tools (fetch, read_file, web_search, etc.) are auto-approved at the Rust level. Side-effect tools (exec, write_file, delete_file) emit a `ToolRequest` event → the frontend shows a risk-classified approval modal → user decides → `engine_approve_tool` resolves.

---

## Human-in-the-Loop (HIL) Approval

Tool calls are classified into two tiers at the Rust engine level:

**Auto-approved (no modal):** Read-only and informational tools — `fetch`, `read_file`, `list_directory`, `web_search`, `web_read`, `memory_search`, `soul_read`, `soul_write`, `self_info`, `email_read`, `slack_read`, `create_task`, `image_generate`, etc.

**Requires user approval (modal shown):** Side-effect tools — `exec`, `write_file`, `append_file`, `delete_file`, and all trading write operations (swaps, transfers). The approval modal classifies each request by risk:

| Risk Level | Behavior | Example |
|------------|----------|---------|
| **Critical** | Auto-denied by default; red modal if auto-deny disabled, user must type "ALLOW" | `sudo rm -rf /`, `curl \| bash` |
| **High** | Orange warning modal | `chmod 777`, `kill -9` |
| **Medium** | Yellow caution modal | `npm install`, outbound HTTP |
| **Low** | Standard approval modal | unknown exec commands |
| **Safe** | Auto-approved if matches allowlist (90+ default patterns) | `git status`, `ls`, `cat` |

### Danger Pattern Detection

30+ patterns across multiple categories:

- **Privilege escalation** — `sudo`, `su`, `doas`, `pkexec`, `runas`
- **Destructive deletion** — `rm -rf /`, `rm -rf ~`, `rm -rf /*`
- **Permission exposure** — `chmod 777`, `chmod -R 777`
- **Disk destruction** — `dd if=`, `mkfs`, `fdisk`
- **Remote code execution** — `curl | sh`, `wget | bash`
- **Code injection** — `eval`, `exec` with untrusted input
- **Process termination** — `kill -9 1`, `killall`
- **Firewall disabling** — `iptables -F`, `ufw disable`
- **Account modification** — `passwd`, `chpasswd`, `usermod`
- **Network exfiltration** — `curl | cat`, `scp` outbound, `/dev/tcp`

### Command Allowlist / Denylist

Configurable regex patterns in Settings:
- **Allowlist** — ~90+ default safe patterns (git, npm, node, python, ls, cat, etc.) — auto-approved
- **Denylist** — default dangerous patterns — auto-denied
- **Custom rules** — users can add their own regex patterns
- Patterns validated before saving; invalid regex is rejected

### Session Override

Timed "allow all" mode with configurable duration (30min, 1hr, 2hr). Privilege escalation commands remain blocked even during override. Auto-expires. Cancellable from Settings banner.

### Trading Approval Policy

Financial tools (swaps, transfers) require HIL approval by default. A configurable trading policy can auto-approve within limits:

- **Max trade size** — per-transaction USD cap
- **Daily loss limit** — cumulative daily spending cap
- **Allowed pairs** — whitelist of tradeable pairs
- **Transfer toggle + cap** — opt-in with per-transfer limit
- Applies to all chains: Coinbase, Solana (Jupiter), EVM DEX (Uniswap)
- Read-only trading tools (balances, quotes, portfolio, prices) are always auto-approved

---

## Prompt Injection Detection

Dual implementation (TypeScript + Rust) scanning for 30+ injection patterns across 4 severity levels. Detects attempts to override system prompts, extract secrets, or manipulate agent behavior.

---

## Container Sandboxing

Docker-based sandboxing via the `bollard` crate:
- `cap_drop ALL` — no Linux capabilities
- Memory and CPU limits
- Network isolation configurable
- Configurable per-agent sandbox policies

---

## Credential Security

### OS Keychain
All sensitive credentials stored in the platform keychain:
- macOS: Keychain
- Linux: libsecret
- Windows: Credential Manager

Config files contain keychain references, never plaintext secrets.

### Database Encryption
Sensitive database fields encrypted with AES-256-GCM via Web Crypto API. The encryption key is derived from the OS keychain.

### Credential Audit Trail
Every credential access is logged to `credential_activity_log` with:
- Action performed
- Tool that requested access
- Whether access was allowed or denied
- Timestamp

---

## Filesystem Sandboxing

### Tauri Scope
Filesystem access scoped via Tauri capabilities (`capabilities/default.json`). Shell access limited to `open` command only.

### Sensitive Path Blocking
20+ sensitive paths blocked from project file browsing:
`~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.kube`, `~/.docker`, `/etc`, `/root`, `/proc`, `/sys`, `/dev`, filesystem root, home directory root.

### Per-Project Scope
File operations validated against the active project root. Path traversal blocked. Violations logged to the security audit.

### Read-Only Mode
Toggle in Security Policies blocks all agent filesystem write tools (create, edit, delete, move, chmod, etc.).

---

## Channel Access Control

Each of the 10 channel bridges supports:
- **DM policy** — pairing / allowlist / open
- **Pairing approval** — new users send a request → approved in Pawz → confirmation sent back
- **Per-channel allowlist** — specific user IDs
- **Per-agent routing** — configure which agents handle which channels

---

## Network Security

### Content Security Policy (CSP)
Restrictive CSP in `tauri.conf.json`:
- `default-src 'self'`
- `script-src 'self'` — no external scripts
- `connect-src 'self'` + localhost WebSocket only
- `object-src 'none'`
- `frame-ancestors 'none'`

### Network Request Auditing
Outbound tool calls are inspected for exfiltration patterns. URL extraction and domain analysis with audit logging.

### Outbound Domain Allowlist
Configurable allow/block lists with wildcard subdomain matching. Enforced in the `execute_fetch` tool handler. Test URL button in settings.

---

## Skill Vetting

Before every skill install:
1. **Safety confirmation modal** — shows security checks
2. **Known-safe list** — built-in set of community-vetted skill names
3. **npm registry risk intelligence** — fetches download count, last publish date, deprecation status, maintainer count, license
4. **Risk score display** — visual risk panel in the confirmation dialog
5. **Post-install sandbox check** — verifies skill metadata for suspicious tool registrations (`exec`, `shell`, `eval`, `spawn`)

---

## Audit Dashboard

Unified security audit log (`security_audit_log` table) capturing all security-relevant events:
- Event type, risk level, tool name
- Command details
- Session context
- Decision (allowed/denied)
- Matched pattern

Filterable by type, date, and severity. Export to JSON or CSV.

---

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it responsibly by emailing the maintainer directly rather than opening a public issue. See the repository's contact information for details.
