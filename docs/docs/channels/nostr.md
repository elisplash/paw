---
sidebar_position: 9
title: Nostr
---

# Nostr

Connect to the Nostr decentralized network so users can chat with your agents via any Nostr client.

## Setup

1. Generate a Nostr keypair for your bot (or use an existing one)
2. In Pawz, go to **Settings → Channels**
3. Select **Nostr**
4. Enter:
   - **Private key** — bot's nsec key
   - **Relays** — comma-separated relay URLs (e.g., `wss://relay.damus.io,wss://nos.lol`)
5. Start the channel

## Configuration

| Field | Required | Description |
|-------|----------|-------------|
| Private key | Yes | Bot's Nostr private key (nsec) |
| Relays | Yes | Relay URLs to connect to |
| DM policy | Yes | Who can message the bot |
| Allowed users | For allowlist | Nostr public keys (npub) |

## Features

- NIP-04 encrypted DMs
- Per-user sessions with memory
- Prompt injection scanning
- Agent routing via channel routing rules
- Multi-relay support

## Tips

- Use multiple relays for reliability
- The bot's public key (npub) is derived from the private key automatically
- Users can find your bot by its npub on any Nostr client
- Consider publishing a NIP-05 identifier for discoverability
