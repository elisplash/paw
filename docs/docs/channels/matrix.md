---
sidebar_position: 5
title: Matrix
---

# Matrix

Connect to a Matrix homeserver so users can chat with your agents via any Matrix client (Element, FluffyChat, etc.).

## Setup

1. Create a bot account on your Matrix homeserver
2. Note the homeserver URL, username, and password
3. In Pawz, go to **Settings → Channels**
4. Select **Matrix**
5. Enter:
   - **Homeserver URL** — e.g., `https://matrix.org`
   - **Username** — bot's Matrix ID (e.g., `@pawz-bot:matrix.org`)
   - **Password** — bot account password
6. Start the channel

## Configuration

| Field | Required | Description |
|-------|----------|-------------|
| Homeserver URL | Yes | Matrix server URL |
| Username | Yes | Bot's Matrix user ID |
| Password | Yes | Bot account password |
| DM policy | Yes | Who can message the bot |
| Allowed users | For allowlist | Matrix user IDs |

## Features

- DMs and room messages
- End-to-end encryption support (if enabled on the room)
- Per-user sessions with memory
- Prompt injection scanning
- Agent routing via channel routing rules

## Tips

- For self-hosted Matrix (Synapse, Dendrite), use your server's URL
- Invite the bot to rooms where you want it active
- Use Element or any Matrix client to test
