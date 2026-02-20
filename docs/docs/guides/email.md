---
sidebar_position: 8
title: Email
---

# Email

Pawz can read and send email through your existing accounts using Himalaya, an IMAP/SMTP CLI client.

## Prerequisites

Install [Himalaya](https://github.com/pimalaya/himalaya):

```bash
# macOS
brew install himalaya

# Linux
cargo install himalaya
```

## Configuration

Himalaya uses a TOML config file at `~/.config/himalaya/config.toml`:

```toml
[accounts.personal]
email = "you@gmail.com"
display-name = "Your Name"

[accounts.personal.imap]
host = "imap.gmail.com"
port = 993
login = "you@gmail.com"
passwd.cmd = "security find-generic-password -s gmail -w"

[accounts.personal.smtp]
host = "smtp.gmail.com"
port = 465
login = "you@gmail.com"
passwd.cmd = "security find-generic-password -s gmail -w"
```

## Skills setup

Enable the **Email** skill in **Settings → Skills** and provide:

| Credential | Example |
|-----------|---------|
| SMTP_HOST | smtp.gmail.com |
| SMTP_PORT | 465 |
| SMTP_USER | you@gmail.com |
| SMTP_PASS | your app password |
| IMAP_HOST | imap.gmail.com |
| IMAP_USER | you@gmail.com |
| IMAP_PASS | your app password |

:::tip
For Gmail, use an [App Password](https://support.google.com/accounts/answer/185833) — not your regular password.
:::

## Agent tools

With the Email skill enabled, agents get:

| Tool | Approval | Description |
|------|----------|-------------|
| `email_read` | Auto | Read inbox, search emails |
| `email_send` | **HIL required** | Send emails (human must approve) |

## Mail permissions

Each account can have granular permissions:

| Permission | Description |
|-----------|-------------|
| `read` | Allow reading emails |
| `send` | Allow sending emails |
| `delete` | Allow deleting emails |
| `manage` | Allow folder management |

## Provider icons

The mail view auto-detects your provider and shows the appropriate icon:

| Provider | Icon |
|----------|------|
| Gmail | G |
| Outlook | O |
| Yahoo | Y |
| iCloud | iC |
| Fastmail | FM |
| Other | M |

## Channel integration

Email can also be set up as a channel — see [Channel Routing](../channels/overview) for details.
