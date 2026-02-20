---
sidebar_position: 12
title: Browser Sandbox
---

# Browser Sandbox

The browser sandbox lets agents browse the web in a controlled, auditable environment.

## Configuration

Go to **Settings → Advanced** to configure:

| Setting | Default | Description |
|---------|---------|-------------|
| **Headless** | On | Run browser without visible window |
| **Auto-close tabs** | On | Close tabs after agent finishes |
| **Idle timeout** | 300s | Kill browser after inactivity |

## Profiles

Create isolated browser profiles for different purposes:

- Each profile has its own cookies, storage, and history
- Agents can switch between profiles
- Prevents cross-contamination between tasks

## Network policy

Control which domains agents can access:

### Default allowed domains

| Domain | Purpose |
|--------|---------|
| `api.openai.com` | AI provider |
| `api.anthropic.com` | AI provider |
| `generativelanguage.googleapis.com` | AI provider |
| `openrouter.ai` | AI provider |
| `api.elevenlabs.io` | TTS provider |
| `duckduckgo.com` | Web search |
| `html.duckduckgo.com` | Web search |
| `api.coinbase.com` | Trading |
| `localhost` | Local services |

### Default blocked domains

| Domain | Reason |
|--------|--------|
| `pastebin.com` | Data exfiltration risk |
| `transfer.sh` | File sharing risk |
| `file.io` | File sharing risk |
| `0x0.st` | Anonymous upload risk |

### Custom rules

Add your own allowed or blocked domains in the browser settings. Blocked domains take priority over allowed.

### Request logging

Enable **log requests** to see all network requests the browser makes. Recent requests are shown in the settings panel for auditing.

## Agent tools

| Tool | Description |
|------|-------------|
| `web_browse` | Navigate to a URL and interact with the page |
| `web_screenshot` | Capture a screenshot of the current page |
| `web_read` | Extract text content from a page |

## Security

- Network policy is enforced at the browser level — agents cannot bypass it
- All browser activity is contained within the profile
- Headless mode prevents agents from displaying arbitrary content
- Idle timeout prevents runaway browser sessions
