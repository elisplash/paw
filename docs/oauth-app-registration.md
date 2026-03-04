# OAuth App Registration Guide

> **Purpose**: Register OAuth apps on each platform to get real Client IDs for OpenPawz one-click connect.
>
> **Key fact**: PKCE Client IDs are **public** (not secrets). They're safe to commit and ship in the binary.
>
> **Build-time injection**: Set `OPENPAWZ_<SERVICE>_CLIENT_ID` env vars before `cargo build`.

---

## Quick Reference

| Service      | Env Variable                      | Portal URL                                              | Approval |
|-------------|-----------------------------------|---------------------------------------------------------|----------|
| GitHub      | `OPENPAWZ_GITHUB_CLIENT_ID`      | https://github.com/organizations/OpenPawz/settings/applications | Instant  |
| Google      | `OPENPAWZ_GOOGLE_CLIENT_ID`      | https://console.cloud.google.com/apis/credentials        | Instant* |
| Discord     | `OPENPAWZ_DISCORD_CLIENT_ID`     | https://discord.com/developers/applications              | Instant  |
| Slack       | `OPENPAWZ_SLACK_CLIENT_ID`       | https://api.slack.com/apps                               | Review   |
| Notion      | `OPENPAWZ_NOTION_CLIENT_ID`      | https://www.notion.so/my-integrations                    | Instant  |
| Spotify     | `OPENPAWZ_SPOTIFY_CLIENT_ID`     | https://developer.spotify.com/dashboard                  | Instant  |
| Dropbox     | `OPENPAWZ_DROPBOX_CLIENT_ID`     | https://www.dropbox.com/developers/apps                  | Instant  |
| Linear      | `OPENPAWZ_LINEAR_CLIENT_ID`      | https://linear.app/settings/api                          | Instant  |
| Figma       | `OPENPAWZ_FIGMA_CLIENT_ID`       | https://www.figma.com/developers/apps                    | Instant  |
| Reddit      | `OPENPAWZ_REDDIT_CLIENT_ID`      | https://www.reddit.com/prefs/apps                        | Instant  |

\* Google requires verification for >100 users. Unverified apps work for testing with a consent warning.

---

## Build Command

```bash
# Set all Client IDs, then build
export OPENPAWZ_GITHUB_CLIENT_ID="Ov23li..."
export OPENPAWZ_GOOGLE_CLIENT_ID="123456789-abc.apps.googleusercontent.com"
export OPENPAWZ_DISCORD_CLIENT_ID="1234567890"
# ... etc
cargo tauri build
```

Or use a `.env.build` file (gitignored) and source before building:
```bash
source .env.build && cargo tauri build
```

---

## Per-Platform Registration Steps

### 1. GitHub (Instant)

1. Go to **https://github.com/organizations/OpenPawz/settings/applications/new**
   - Or for personal: https://github.com/settings/applications/new
2. Fill in:
   - **Application name**: `OpenPawz`
   - **Homepage URL**: `https://openpawz.com`
   - **Authorization callback URL**: `http://127.0.0.1/callback`
     - Note: GitHub allows any port on localhost for native apps
   - **Enable Device Flow**: No (we use PKCE redirect)
3. Click **Register application**
4. Copy the **Client ID** (starts with `Ov23li` for OAuth Apps)
5. **No client secret needed** — PKCE eliminates it

```bash
export OPENPAWZ_GITHUB_CLIENT_ID="Ov23liXXXXXXXXXXXXXX"
```

> **GitHub Note**: GitHub OAuth Apps use the classic flow. For fine-grained permissions,
> consider creating a **GitHub App** instead (same OAuth flow, but with installation-level scopes).

---

### 2. Google (Instant for Testing, Verification for Production)

1. Go to **https://console.cloud.google.com**
2. Create a new project: **OpenPawz**
3. Go to **APIs & Services → OAuth consent screen**
   - Select **External** user type
   - Fill in app name, support email, developer contact email
   - Add scopes:
     - `https://www.googleapis.com/auth/gmail.readonly`
     - `https://www.googleapis.com/auth/calendar.readonly`
     - `https://www.googleapis.com/auth/drive.readonly`
     - `https://www.googleapis.com/auth/gmail.send` (sensitive)
     - `https://www.googleapis.com/auth/calendar` (sensitive)
     - `https://www.googleapis.com/auth/drive` (sensitive)
     - `https://www.googleapis.com/auth/spreadsheets` (sensitive)
4. Go to **APIs & Services → Credentials → Create Credentials → OAuth client ID**
   - Application type: **Desktop app**
   - Name: `OpenPawz Desktop`
5. Copy the **Client ID** (format: `123456789-xxxx.apps.googleusercontent.com`)

```bash
export OPENPAWZ_GOOGLE_CLIENT_ID="123456789-xxxx.apps.googleusercontent.com"
```

> **Google Note**: Desktop apps using PKCE don't need a client secret.
> Unverified apps show a warning screen but work for <100 users.
> Submit for verification when ready for production.

---

### 3. Discord (Instant)

1. Go to **https://discord.com/developers/applications**
2. Click **New Application** → name it `OpenPawz`
3. Go to **OAuth2** tab
4. Add redirect: `http://127.0.0.1/callback`
   - Discord allows any port on localhost/127.0.0.1
5. Copy the **Client ID** from the General Information tab

```bash
export OPENPAWZ_DISCORD_CLIENT_ID="1234567890123456789"
```

> **Discord Note**: For bot functionality, also enable the **Bot** section
> and turn on **Message Content Intent** under Privileged Gateway Intents.

---

### 4. Slack (Requires Review for Distribution)

1. Go to **https://api.slack.com/apps** → **Create New App** → **From scratch**
2. Name: `OpenPawz`, pick a development workspace
3. Go to **OAuth & Permissions**
   - Add redirect URL: `http://127.0.0.1/callback`
   - Add **Bot Token Scopes**:
     - `channels:read`, `groups:read`, `users:read`, `team:read` (default)
     - `chat:write`, `files:write`, `channels:manage` (write)
4. Go to **Basic Information** → copy the **Client ID**

```bash
export OPENPAWZ_SLACK_CLIENT_ID="1234567890.1234567890123"
```

> **Slack Note**: For distributing to other workspaces, submit for
> **App Directory review** under Manage Distribution. For internal use,
> install directly to your workspace.

---

### 5. Notion (Instant)

1. Go to **https://www.notion.so/my-integrations** → **New integration**
2. Name: `OpenPawz`
3. Select type: **Public** (required for OAuth)
4. Under **OAuth Domain & URIs**:
   - Redirect URI: `http://127.0.0.1/callback`
5. Copy the **OAuth client ID**

```bash
export OPENPAWZ_NOTION_CLIENT_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

> **Notion Note**: Notion OAuth doesn't use scopes in the auth URL.
> Permissions are configured in the integration settings.
> Users select which pages/databases to share during authorization.

---

### 6. Spotify (Instant)

1. Go to **https://developer.spotify.com/dashboard** → **Create app**
2. Name: `OpenPawz`, Description: `AI assistant with Spotify integration`
3. Redirect URI: `http://127.0.0.1/callback`
4. Select **Web API** and **Web Playback SDK**
5. Copy the **Client ID**

```bash
export OPENPAWZ_SPOTIFY_CLIENT_ID="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

> **Spotify Note**: Extended quota mode needed for >25 users.
> Apply via the Spotify Developer Dashboard.

---

### 7. Dropbox (Instant)

1. Go to **https://www.dropbox.com/developers/apps** → **Create app**
2. Choose: **Scoped access** → **Full Dropbox**
3. Name: `OpenPawz`
4. Under **OAuth 2** settings:
   - Add redirect URI: `http://127.0.0.1/callback`
   - Set **Allow implicit grant**: No
   - Enable **PKCE**: Yes
5. Copy the **App key** (= Client ID)

```bash
export OPENPAWZ_DROPBOX_CLIENT_ID="xxxxxxxxxxxxxxx"
```

---

### 8. Linear (Instant)

1. Go to **https://linear.app/settings/api** → **OAuth Applications** → **New**
2. Name: `OpenPawz`
3. Redirect URI: `http://127.0.0.1/callback`
4. Select scopes: `read`, `write`, `issues:create`
5. Copy the **Client ID**

```bash
export OPENPAWZ_LINEAR_CLIENT_ID="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

---

### 9. Figma (Instant)

1. Go to **https://www.figma.com/developers/apps** → **Create a new app**
2. Name: `OpenPawz`
3. Redirect URI: `http://127.0.0.1/callback`
4. Copy the **Client ID**

```bash
export OPENPAWZ_FIGMA_CLIENT_ID="xxxxxxxxxxxxxxx"
```

> **Figma Note**: Figma OAuth uses the `state` parameter for CSRF
> (already handled by our PKCE flow).

---

### 10. Reddit (Instant)

1. Go to **https://www.reddit.com/prefs/apps** → **create another app...**
2. Select type: **installed app** (for native/desktop)
3. Name: `OpenPawz`
4. Redirect URI: `http://127.0.0.1/callback`
5. Copy the **client id** (shown under the app name)

```bash
export OPENPAWZ_REDDIT_CLIENT_ID="XXXXXXXXXXXXXX"
```

> **Reddit Note**: Reddit uses "installed app" type for PKCE.
> Rate limited to 60 requests/minute per OAuth token.

---

## CI/CD Setup

### GitHub Actions

Add Client IDs as **non-secret** repository variables (they're public):

```yaml
# .github/workflows/build.yml
env:
  OPENPAWZ_GITHUB_CLIENT_ID: ${{ vars.OPENPAWZ_GITHUB_CLIENT_ID }}
  OPENPAWZ_GOOGLE_CLIENT_ID: ${{ vars.OPENPAWZ_GOOGLE_CLIENT_ID }}
  OPENPAWZ_DISCORD_CLIENT_ID: ${{ vars.OPENPAWZ_DISCORD_CLIENT_ID }}
  OPENPAWZ_SLACK_CLIENT_ID: ${{ vars.OPENPAWZ_SLACK_CLIENT_ID }}
  OPENPAWZ_NOTION_CLIENT_ID: ${{ vars.OPENPAWZ_NOTION_CLIENT_ID }}
  OPENPAWZ_SPOTIFY_CLIENT_ID: ${{ vars.OPENPAWZ_SPOTIFY_CLIENT_ID }}
  OPENPAWZ_DROPBOX_CLIENT_ID: ${{ vars.OPENPAWZ_DROPBOX_CLIENT_ID }}
  OPENPAWZ_LINEAR_CLIENT_ID: ${{ vars.OPENPAWZ_LINEAR_CLIENT_ID }}
  OPENPAWZ_FIGMA_CLIENT_ID: ${{ vars.OPENPAWZ_FIGMA_CLIENT_ID }}
  OPENPAWZ_REDDIT_CLIENT_ID: ${{ vars.OPENPAWZ_REDDIT_CLIENT_ID }}
```

### Local Development

Create `.env.build` (already gitignored):
```bash
# OAuth Client IDs (PKCE — public, not secrets)
export OPENPAWZ_GITHUB_CLIENT_ID="Ov23li..."
export OPENPAWZ_GOOGLE_CLIENT_ID="123456789-xxxx.apps.googleusercontent.com"
# ... fill in as you register each app
```

Build with: `source .env.build && cargo tauri build`

---

## Redirect URI Notes

All platforms are configured with `http://127.0.0.1/callback` as the redirect URI.
OpenPawz uses an **ephemeral port** (binds to port 0), so the actual callback URL is
`http://127.0.0.1:{random-port}/callback`.

Most platforms accept any port on `127.0.0.1` for native/desktop apps:
- ✅ GitHub, Google, Discord, Notion, Spotify, Linear — any localhost port
- ⚠️ Slack — may require exact port match; test with your app config
- ⚠️ Reddit — register as "installed app" type which allows localhost
- ⚠️ Dropbox — enable PKCE explicitly in app settings

If a platform requires an exact port, update the OAuth flow in
`src-tauri/src/engine/oauth.rs` to bind to a fixed port for that service.

---

## Verification Checklist

After registering each app, verify the flow works:

- [ ] Client ID set in env var
- [ ] `cargo build` succeeds (Client ID compiled in)
- [ ] Click "Connect {Service}" in OpenPawz UI
- [ ] Browser opens to correct authorization URL
- [ ] Auth URL contains correct Client ID
- [ ] After authorization, callback fires and tokens are stored
- [ ] `engine_oauth_status` shows connected = true
- [ ] Agent can use the service's tools
