---
sidebar_position: 1
title: Agents
---

# Agents

Agents are the core of Pawz — autonomous AI personas that chat, use tools, remember context, and collaborate.

## Creating an agent

1. Go to **Agents** in the sidebar
2. Click **New Agent**
3. Fill in the basics:
   - **Name** — displayed in chat and dock
   - **Avatar** — choose from 50 sprite avatars
   - **Color** — pick from 7 accent colors
   - **Bio** — short description of what this agent does

## Templates

Templates pre-configure skills and personality for common use cases:

| Template | Skills | Tone | Initiative | Detail |
|----------|--------|------|------------|--------|
| **General** | web_search, web_fetch, read, write | balanced | balanced | balanced |
| **Research** | web_search, web_fetch, read, write, browser | formal | proactive | thorough |
| **Creative** | web_search, web_fetch, read, write | casual | proactive | balanced |
| **Technical** | web_search, web_fetch, read, write, exec, edit | balanced | reactive | thorough |
| **Custom** | you choose | you choose | you choose | you choose |

## Personality settings

Each agent has three personality axes:

| Axis | Options | Description |
|------|---------|-------------|
| **Tone** | casual / balanced / formal | How the agent sounds |
| **Initiative** | reactive / balanced / proactive | How much the agent does without being asked |
| **Detail** | brief / balanced / thorough | Response length and depth |

## Model selection

Each agent can use a different model:

- **Default** — uses the engine's default model
- **Specific model** — pick from the well-known model catalog or type any model name

Well-known models include:
- **Gemini**: gemini-2.5-pro, gemini-2.5-flash, gemini-2.0-flash
- **Anthropic**: claude-sonnet-4, claude-haiku-4, claude-opus-4
- **OpenAI**: gpt-4o, gpt-4o-mini, o1, o3-mini

## Skills

Toggle which tools an agent can use:

`web_search` · `web_fetch` · `browser` · `read` · `write` · `exec` · `image` · `memory_store` · `cron` · `message`

See the [Skills guide](./skills) for the full skill vault.

## Soul files

Every agent gets three markdown files that define its identity:

| File | Purpose |
|------|---------|
| **IDENTITY.md** | Name, emoji, vibe, avatar description |
| **SOUL.md** | Personality, communication style, boundaries |
| **USER.md** | Who the user is, preferences, how to address them |

Additional standard files:
- **AGENTS.md** — Operating rules, priorities, memory usage
- **TOOLS.md** — Notes about local tools and conventions

Edit these in the agent detail view. Changes take effect on the next message.

## Agent dock

The dock is a floating tray at the bottom-right of the screen:

- Shows all your agents as circular avatars
- Click an avatar to open a **mini-chat** popup
- Unread badge shows new messages
- Collapse/expand with the toggle button
- Tooltips show agent name on hover

## Mini-chat

Each agent has a Messenger-style chat window:

- Opens as a popup from the dock
- Real-time streaming responses
- Full tool execution with approval flow
- Independent session per agent
- Drag to reposition

## System prompt

For advanced users: override the agent's system prompt entirely. This replaces all auto-generated instructions. Use this when you need full control.

## Boundaries

Add custom rules the agent must follow, like:

- "Never share API keys"
- "Always ask before deleting files"
- "Keep responses under 200 words"
