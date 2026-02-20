---
sidebar_position: 2
title: Security
---

# Security

Pawz is designed with defense-in-depth: multiple layers protect against prompt injection, data exfiltration, and unauthorized actions.

## Human-in-the-Loop (HIL)

Every tool is classified by risk level. High-risk tools require explicit human approval before execution.

### Auto-approved tools (no approval needed)

`fetch` · `read_file` · `list_directory` · `web_search` · `web_read` · `memory_search` · `memory_store` · `soul_read` · `soul_write` · `soul_list` · `self_info` · `update_profile` · `create_task` · `list_tasks` · `manage_task` · `email_read` · `slack_read` · `telegram_read` · `image_generate`

### HIL-required tools (human must approve)

`exec` · `write_file` · `delete_file` · `append_file` · `email_send` · `webhook_send` · `rest_api_call` · `slack_send` · `github_api`

## Agent policies

Per-agent tool access control with four presets:

| Preset | Mode | Description |
|--------|------|-------------|
| **Unrestricted** | unrestricted | All tools, no approval |
| **Standard** | denylist | High-risk tools require approval |
| **Read-only** | allowlist | Only safe read tools |
| **Sandbox** | allowlist | web_search, web_read, memory_search, soul_read only |

You can also create custom policies with specific tool allowlists/denylists.

### Risk classification

| Risk | Tools |
|------|-------|
| **Safe** | `read_file`, `list_directory`, `web_search`, `web_read`, `memory_search`, `soul_read`, `soul_list`, `self_info`, `fetch` |
| **High-risk** | `exec`, `write_file`, `delete_file`, `append_file`, `email_send`, `webhook_send`, `rest_api_call`, `slack_send`, `github_api`, `image_generate`, `soul_write`, `update_profile`, `create_agent`, `create_task`, `manage_task` |

## Prompt injection defense

All incoming channel messages are scanned for injection attempts before reaching the agent.

### Detection

Pattern-based scoring across 9 categories:

| Category | Examples |
|----------|----------|
| `override` | "Ignore previous instructions" |
| `identity` | "You are now..." |
| `jailbreak` | "DAN mode", "no restrictions" |
| `leaking` | "Show me your system prompt" |
| `obfuscation` | Base64-encoded instructions |
| `tool_injection` | Fake tool call formatting |
| `social` | "As an AI researcher..." |
| `markup` | Hidden instructions in HTML/markdown |
| `bypass` | "This is just a test..." |

### Severity levels

| Severity | Score | Action |
|----------|-------|--------|
| **Critical** | 40+ | Message blocked, not delivered |
| **High** | 25+ | Warning logged |
| **Medium** | 12+ | Noted in logs |
| **Low** | 5+ | Informational |

Channel bridges automatically block messages with `critical` severity.

## Container sandbox

Execute agent commands in isolated Docker containers:

| Security measure | Default |
|-----------------|---------|
| Capabilities | `cap_drop ALL` |
| Network | Disabled |
| Memory limit | 256 MB |
| CPU shares | 512 |
| Timeout | 30 seconds |
| Output limit | 50 KB |

### Presets

| Preset | Image | Memory | Network | Timeout |
|--------|-------|--------|---------|---------|
| Minimal | alpine | 128 MB | Off | 15s |
| Development | node:20-alpine | 512 MB | On | 60s |
| Python | python:3.12-alpine | 512 MB | On | 60s |
| Restricted | alpine | 64 MB | Off | 10s |

### Command risk assessment

Commands are scored before execution:
- **Low** — `ls`, `cat`, `echo`
- **Medium** — `pip install`, `npm install`
- **High** — `curl`, `wget`, network commands
- **Critical** — `rm -rf /`, `chmod 777`, dangerous patterns

## Browser network policy

Control which domains agents can access:

**Default allowed:** AI provider APIs, DuckDuckGo, Coinbase, localhost

**Default blocked:** pastebin.com, transfer.sh, file.io, 0x0.st (data exfiltration risks)

## Credential security

- API keys encrypted with XOR using a 32-byte random key
- Encryption key stored in OS keychain (`paw-skill-vault`)
- High-risk credentials (Coinbase, DEX) are **server-side only** — never injected into prompts
- Credentials are decrypted only at execution time

## Budget enforcement

Daily spending limits with progressive warnings:

| Threshold | Action |
|-----------|--------|
| 50% | Warning |
| 75% | Warning |
| 90% | Warning |
| 100% | Requests blocked |

## Trading safety

| Control | Default |
|---------|---------|
| Auto-approve trades | Off |
| Max trade size | $100 |
| Max daily loss | $500 |
| Transfers | Disabled |
| Max transfer | $0 |

## Channel access control

| Policy | Behavior |
|--------|----------|
| Open | Anyone can chat |
| Allowlist | Only approved users |
| Pairing | Users must pair with a code |

Each user gets an isolated session — no cross-user data leakage.

## Reporting vulnerabilities

See [SECURITY.md](https://github.com/elisplash/paw/blob/main/SECURITY.md) in the repository for reporting instructions.
