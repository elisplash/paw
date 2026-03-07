# OAuth App Registration Guide

> **Purpose**: Step-by-step instructions for registering OAuth apps on every platform.
> Use this alongside the [checklist](oauth-registration-checklist.md) to track progress.
>
> **Key fact**: PKCE Client IDs are **public** (not secrets). They're safe to commit and ship in the binary.
> **Client Secrets** (when issued) are stored in Bitwarden, never committed.

---

## Standard Values (Use for Every Service)

| Field | What to Enter |
|-------|--------------|
| **App name** | `OpenPawz` |
| **Description** | `AI-powered desktop assistant` |
| **App type** | Native / Desktop / Installed (pick whichever is NOT "web server") |
| **Website / Homepage** | `https://openpawz.com` |
| **Redirect URI** | `http://127.0.0.1:0/callback` |
| **Fallback Redirect URI** | `http://localhost:19284/callback` (use if portal rejects port 0) |
| **Privacy Policy** | `https://openpawz.com/privacy` |
| **Terms of Service** | `https://openpawz.com/terms` |
| **Contact Email** | `dev@openpawz.ai` |

---

## Field Name Translations

Different portals use different names for the same thing:

| What we call it | Other names you'll see on portals |
|----------------|----------------------------------|
| **Client ID** | App ID, Application ID, Consumer Key, API Key, App Key |
| **Client Secret** | App Secret, Consumer Secret, API Secret, Secret Key |
| **Redirect URI** | Callback URL, Redirect URL, OAuth redirect, Return URL, Authorization callback URL |
| **Scopes** | Permissions, Access levels, API permissions |
| **Native / Desktop** | Installed app, Public client, Mobile app, SPA, Single-page app |

---

## After Each Registration

1. Open Bitwarden → find the entry for that service
2. Paste the **Client ID** into the `client_id` custom field
3. Paste the **Client Secret** (if issued) into the `client_secret` custom field
4. Save the entry
5. Mark the row ✅ in the [checklist](oauth-registration-checklist.md)

---

## Per-Service Walkthroughs

### Productivity & Project Management

#### Asana
1. Go to https://app.asana.com/0/developer-console
2. Click **"Create new app"**
3. **App name:** `OpenPawz` → click **"Create app"**
4. In the sidebar click **"OAuth"**
5. **Redirect URL:** `http://127.0.0.1:0/callback` → **"Add"**
6. Copy **Client ID** and **Client secret** from this page
7. Save in Bitwarden

#### Basecamp
1. Go to https://launchpad.37signals.com/integrations
2. Click **"Register your application"** (sign up for 37signals ID first if needed)
3. **Name:** `OpenPawz`, **Company:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### ClickUp
1. Log in to ClickUp → go to https://app.clickup.com/settings/integrations
2. Scroll to **"ClickUp API"** → click **"Create an App"**
3. **App Name:** `OpenPawz`
4. **Redirect URL(s):** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Figma
1. Go to https://www.figma.com/developers/apps → **"Create a new app"**
2. **App name:** `OpenPawz`, **Website URL:** `https://openpawz.com`
3. **Callback URL:** `http://127.0.0.1:0/callback`
4. Click **"Save"**
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Harvest
1. Go to https://id.getharvest.com/oauth2/access_tokens
2. Click **"Create New OAuth2 Application"**
3. **Name:** `OpenPawz`, **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Linear
1. Go to https://linear.app/settings/api → **"OAuth Applications"** tab
2. Click **"Create new"**
3. **Application name:** `OpenPawz`
4. **Redirect callback URLs:** `http://127.0.0.1:0/callback`
5. **Developer URL:** `https://openpawz.com`
6. Click **"Create"**
7. Copy **Client ID** and **Client Secret**
8. Save in Bitwarden

#### Miro
1. Go to https://developers.miro.com → sign in → click **"Your apps"**
2. Click **"Create a new app"**
3. **App name:** `OpenPawz`
4. In the app settings, find **"Redirect URI for OAuth2.0"**
5. Enter `http://127.0.0.1:0/callback`
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Monday.com
1. Go to https://monday.com/developers/apps → **"Create app"**
2. **App Name:** `OpenPawz`
3. In the sidebar, go to **"OAuth & Permissions"**
4. **Redirect URLs:** add `http://127.0.0.1:0/callback`
5. Select the **Scopes** you need (start with `boards:read`, `me:read`)
6. Go to **"Basic Information"** → copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### ProductBoard
1. Go to https://developer.productboard.com → sign in
2. Navigate to your app settings → **"Create app"**
3. **Name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Slack
1. Go to https://api.slack.com/apps → **"Create New App"** → **"From scratch"**
2. **App Name:** `OpenPawz`, pick a workspace
3. In sidebar, click **"OAuth & Permissions"**
4. Under **Redirect URLs**, click **"Add New Redirect URL"**
5. Enter `http://127.0.0.1:0/callback` → **"Add"** → **"Save URLs"**
6. Scroll to **"Scopes"** → add Bot Token Scopes: `chat:write`, `channels:read`, `users:read`
7. Go to **"Basic Information"** → copy **Client ID** and **Client Secret**
8. Save in Bitwarden

#### Teamwork
1. Go to https://developer.teamwork.com → sign in
2. Click **"Create App"**
3. **App name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### TickTick
1. Go to https://developer.ticktick.com/manage → sign in
2. Click **"Create App"** or **"Add Application"**
3. **App Name:** `OpenPawz`
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Timely
1. Go to https://timelyapp.com/developer → sign in
2. **"Create a new app"**
3. **App name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Application ID** (= Client ID) and **Secret**
6. Save in Bitwarden

#### Wrike
1. Go to https://www.wrike.com/apps/api → sign in
2. Click **"Create New"** under API apps
3. **App Name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Canva
1. Go to https://www.canva.com/developers/ → sign in → **"Create an app"**
2. **App name:** `OpenPawz`
3. **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Mural
1. Go to https://developers.mural.co → sign in → **"Create App"**
2. **App Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Envoy
1. Go to https://developers.envoy.com → sign in
2. **"Create Integration"** or **"New App"**
3. **Name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Workable
1. Go to https://developer.workable.com → sign in
2. **"Create App"**
3. **Name:** `OpenPawz`
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

---

### CRM & Sales

#### HubSpot
1. Go to https://developers.hubspot.com → sign up / log in
2. Click **"Create a developer account"** if needed (free)
3. Go to **"Apps"** in the top nav → **"Create app"**
4. **App name:** `OpenPawz`
5. Go to the **"Auth"** tab
6. **Redirect URL:** `http://127.0.0.1:0/callback`
7. Under **Scopes**, add `crm.objects.contacts.read`
8. Copy **Client ID** and **Client Secret** from the Auth tab
9. Save in Bitwarden

#### Salesforce
1. Go to https://developer.salesforce.com → sign up for free Developer Edition
2. Once logged in, click gear icon → **"Setup"**
3. In left search bar, type **"App Manager"** → click it
4. Click **"New Connected App"**
5. **Connected App Name:** `OpenPawz`, **Contact Email:** `dev@openpawz.ai`
6. Check **"Enable OAuth Settings"**
7. **Callback URL:** `http://127.0.0.1:0/callback`
8. **Selected OAuth Scopes:** add `Full access (full)` and `Perform requests at any time (refresh_token, offline_access)`
9. Click **"Save"** → **"Continue"**
10. Wait 2-10 minutes, then go back to App Manager → find OpenPawz → dropdown → **"View"**
11. Click **"Manage Consumer Details"** → copy **Consumer Key** (= Client ID) and **Consumer Secret**
12. Save in Bitwarden

#### Pipedrive
1. Go to https://developers.pipedrive.com → sign in
2. Go to **"Developer Hub"** → **"Create an app"** → choose **"OAuth"**
3. **App name:** `OpenPawz`
4. **Callback URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Close
1. Go to https://developer.close.com → sign in
2. **"Create App"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Copper
1. Go to https://developer.copper.com → sign in
2. Create a new OAuth app
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Attio
1. Go to https://developers.attio.com → sign in
2. **"Create App"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Zoho
1. Go to https://api-console.zoho.com → sign in or create free Zoho account
2. Click **"Add Client"** → choose **"Server-based Applications"** (or Desktop)
3. **Client Name:** `OpenPawz`, **Homepage URL:** `https://openpawz.com`
4. **Authorized Redirect URI:** `http://127.0.0.1:0/callback`
5. Click **"Create"**
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Zendesk Sell
1. Go to https://developer.zendesk.com → sign in
2. Navigate to API settings → **"OAuth Clients"** → **"Add OAuth Client"**
3. **Name:** `OpenPawz`, **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Wealthbox
1. Go to https://dev.wealthbox.com → sign in
2. **"Create App"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### PreciseFP
1. Go to https://developer.precisefp.com → sign in
2. **"Create Application"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Communication & Social

#### Discord
1. Go to https://discord.com/developers/applications → **"New Application"**
2. **Name:** `OpenPawz` → **"Create"**
3. In sidebar, click **"OAuth2"**
4. Copy **Client ID** and **Client Secret**
5. Under **Redirects**, click **"Add Redirect"** → `http://127.0.0.1:0/callback` → **"Save Changes"**
6. Save in Bitwarden

#### Microsoft 365
> Already registered. Client ID: `e1026883-ecd3-4116-a2dd-49cd43eea191`

If you need to re-register:
1. Go to https://portal.azure.com → **"App registrations"** → **"New registration"**
2. **Name:** `OpenPawz`
3. **Supported account types:** Accounts in any directory + personal accounts
4. **Redirect URI:** Platform = **Public client/native**, URI = `http://127.0.0.1:0/callback`
5. Click **"Register"**
6. Copy **Application (client) ID**
7. Go to **Certificates & secrets** → **"New client secret"** → copy the **Value**
8. Save in Bitwarden

#### Webex
1. Go to https://developer.webex.com/my-apps → **"Create a New App"**
2. Choose **"Integration"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Select scopes: `spark:messages_read`, `spark:rooms_read`
5. Click **"Add Integration"**
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Tumblr
1. Go to https://www.tumblr.com/oauth/apps → **"Register application"**
2. **Application name:** `OpenPawz`, **Default callback URL:** `http://127.0.0.1:0/callback`
3. **Application website:** `https://openpawz.com`
4. Click **"Register"**
5. Copy **OAuth Consumer Key** (= Client ID) and **Secret key**
6. Save in Bitwarden

#### Reddit
1. Go to https://www.reddit.com/prefs/apps → scroll down → **"create another app..."**
2. **name:** `OpenPawz`
3. Select **"installed app"** (the native/desktop type)
4. **redirect uri:** `http://127.0.0.1:0/callback`
5. Click **"create app"**
6. Client ID is the string under the app name
7. Installed apps don't issue a client secret — PKCE handles it
8. Save in Bitwarden

---

### Developer Tools & DevOps

#### GitHub
1. Go to https://github.com/settings/developers → **"OAuth Apps"** tab
2. Click **"New OAuth App"**
3. **Application name:** `OpenPawz`
4. **Homepage URL:** `https://openpawz.com`
5. **Authorization callback URL:** `http://127.0.0.1:0/callback`
6. Click **"Register application"**
7. Copy **Client ID**, then click **"Generate a new client secret"** → copy secret
8. Save in Bitwarden

#### Bitbucket
1. Go to https://bitbucket.org/account/settings/app-authorizations/
2. Click **"Add consumer"**
3. **Name:** `OpenPawz`, **Callback URL:** `http://127.0.0.1:0/callback`
4. **URL:** `https://openpawz.com`
5. Select permissions as needed
6. Click **"Save"**
7. Copy **Key** (= Client ID) and **Secret**
8. Save in Bitwarden

#### Atlassian / Jira
1. Go to https://developer.atlassian.com/console/myapps/
2. Click **"Create"** → **"OAuth 2.0 integration"**
3. **Name:** `OpenPawz` → **"Create"**
4. In sidebar, click **"Authorization"** → **"Add"** next to OAuth 2.0 (3LO)
5. **Callback URL:** `http://127.0.0.1:0/callback` → **"Save changes"**
6. In sidebar, **"Permissions"** → **"Add"** for Jira API → configure scopes
7. Go to **"Settings"** → copy **Client ID**
8. The **Secret** may only be shown once — copy it immediately
9. Save in Bitwarden

#### DigitalOcean
1. Go to https://cloud.digitalocean.com/account/api/applications → **"Register a new OAuth Application"**
2. **Name:** `OpenPawz`, **Homepage URL:** `https://openpawz.com`
3. **Callback URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### PagerDuty
1. Go to https://developer.pagerduty.com/apps → **"Create New App"**
2. **Name:** `OpenPawz`, **Description:** `AI desktop assistant`
3. Add **OAuth 2.0** functionality
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Webflow
1. Go to https://developers.webflow.com → sign in → **"Create App"**
2. **App Name:** `OpenPawz`, **Homepage:** `https://openpawz.com`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Zapier
1. Go to https://developer.zapier.com → sign in
2. **"Create Integration"**
3. **Name:** `OpenPawz`
4. In **Authentication**, select **OAuth 2.0**
5. **Redirect URI:** `http://127.0.0.1:0/callback`
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### WakaTime
1. Go to https://wakatime.com/apps → **"Create a new app"**
2. **App Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **App ID** (= Client ID) and **App Secret**
5. Save in Bitwarden

#### Snowflake
1. Go to your Snowflake account → **Admin** → **Security** → **OAuth**
2. Create a new OAuth integration (SQL command):
   ```sql
   CREATE SECURITY INTEGRATION openpawz
     TYPE = OAUTH
     OAUTH_CLIENT = CUSTOM
     OAUTH_REDIRECT_URI = 'http://127.0.0.1:0/callback'
     ENABLED = TRUE;
   ```
3. Run `DESCRIBE INTEGRATION openpawz;` to get Client ID
4. Save in Bitwarden

> Snowflake is instance-specific — the auth URL uses your account's subdomain.

#### Squarespace
1. Go to https://developers.squarespace.com → sign in → **"Create App"**
2. **App name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Marketing & Email

#### Mailchimp
1. Go to https://admin.mailchimp.com/account/oauth2/ → **"Register And Test OAuth2 Application"**
2. **App name:** `OpenPawz`
3. **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Constant Contact
1. Go to https://app.constantcontact.com/pages/dma/portal/ → **"Create Application"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback` → **"Save"**
4. Copy **API Key** (= Client ID) and **App Secret**
5. Save in Bitwarden

#### Outreach
1. Go to https://developers.outreach.io → sign in → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Application ID** (= Client ID) and **Application Secret**
5. Save in Bitwarden

#### SalesLoft
1. Go to https://developers.salesloft.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Application ID** and **Secret**
5. Save in Bitwarden

#### Keap (Infusionsoft)
1. Go to https://developer.keap.com → sign in → **"Create"**
2. **App Name:** `OpenPawz`
3. Under OAuth settings, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### HighLevel
1. Go to https://marketplace.gohighlevel.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Brex
1. Go to https://developer.brex.com → sign in
2. **"Create App"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Social Media & Video

#### Twitter/X
1. Go to https://developer.twitter.com/en/portal
2. Apply for developer access if you haven't (free — usually approved same day)
3. Once approved, **Projects & Apps** → **"+ Add App"**
4. **App Name:** `OpenPawz`
5. Copy **API Key** and **API Key Secret** (save but we mainly need the OAuth 2.0 Client ID)
6. Go to app **Settings** → **"User authentication settings"** → **"Set up"**
7. **App permissions:** Read and write
8. **Type of App:** Native App
9. **Callback URI:** `http://127.0.0.1:0/callback`
10. **Website URL:** `https://openpawz.com`
11. **"Save"** → copy the **OAuth 2.0 Client ID** shown (this is the one we need)
12. Save in Bitwarden

#### LinkedIn
1. Go to https://www.linkedin.com/developers/apps → **"Create app"**
2. **App name:** `OpenPawz`
3. **LinkedIn Page:** (create a free Company Page for OpenPawz if you don't have one)
4. **App logo:** upload OpenPawz logo
5. **Privacy policy URL:** `https://openpawz.com/privacy`
6. Agree to terms → **"Create app"**
7. Go to **"Auth"** tab → under **Authorized redirect URLs**, add `http://127.0.0.1:0/callback`
8. Copy **Client ID** and **Client Secret**
9. Save in Bitwarden

#### TikTok (Accounts / Personal)
1. Go to https://developers.tiktok.com → sign in → **"My Apps"** → **"Create app"**
2. **App name:** `OpenPawz`
3. Under **Login Kit**, add **Redirect URI:** `http://127.0.0.1:0/callback`
4. Select scopes: `user.info.basic`
5. Submit — app works immediately in **sandbox/dev mode** (limited to test users)
6. Copy **Client Key** (= Client ID) and **Client Secret**
7. Save in Bitwarden

> Submit for App Review later for public access. Not a blocker.

#### TikTok Ads
1. Go to https://business.tiktok.com/apps → **"Create App"**
2. **App name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **App ID** (= Client ID) and **Secret**
5. Save in Bitwarden

#### Snapchat
1. Go to https://business.snapchat.com/developer → sign in → **"Create App"**
2. **App name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Pinterest
1. Go to https://developers.pinterest.com → sign in → **"My apps"** → **"Create app"**
2. **App name:** `OpenPawz`, **Description:** `AI desktop assistant`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **App ID** (= Client ID) and **App secret**
5. Save in Bitwarden

#### Spotify
1. Go to https://developer.spotify.com/dashboard → **"Create app"**
2. **App name:** `OpenPawz`, **App description:** `AI desktop assistant`
3. **Redirect URI:** `http://127.0.0.1:0/callback` → **"Add"**
4. **Which API/SDKs:** check **"Web API"**
5. Agree to terms → **"Save"**
6. Click **"Settings"** → copy **Client ID**, click **"View client secret"** → copy
7. Save in Bitwarden

#### Twitch
1. Go to https://dev.twitch.tv/console/apps → **"Register Your Application"**
2. **Name:** `OpenPawz`
3. **OAuth Redirect URLs:** `http://127.0.0.1:0/callback`
4. **Category:** Application Integration
5. Click **"Create"**
6. Click **"Manage"** on the app → copy **Client ID**, generate **Client Secret**
7. Save in Bitwarden

#### Vimeo
1. Go to https://developer.vimeo.com/apps → **"Create an app"**
2. **App name:** `OpenPawz`, **Description:** `AI desktop assistant`
3. **App URL:** `https://openpawz.com`
4. **Callback URL:** `http://127.0.0.1:0/callback`
5. Copy **Client Identifier** (= Client ID) and **Client Secret**
6. Save in Bitwarden

#### YouTube
> Alias for Google. Already registered under Google Workspace.
> No separate registration needed — uses the same Google OAuth Client ID.

#### Strava
1. Go to https://www.strava.com/settings/api → (create account or log in)
2. **Application Name:** `OpenPawz`
3. **Category:** Utility
4. **Website:** `https://openpawz.com`
5. **Authorization Callback Domain:** `127.0.0.1`
6. Save → copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Osu
1. Go to https://osu.ppy.sh/home/account/edit#oauth → scroll to **"OAuth"**
2. Click **"New OAuth Application"**
3. **Application Name:** `OpenPawz`
4. **Application Callback URL:** `http://127.0.0.1:0/callback`
5. Click **"Register application"**
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Yahoo
1. Go to https://developer.yahoo.com/apps → **"Create an App"**
2. **Application Name:** `OpenPawz`
3. **Application Type:** Installed Application
4. **Redirect URI(s):** `http://127.0.0.1:0/callback`
5. Select API permissions: check **Profiles (Social Directory) — Read**
6. Click **"Create App"**
7. Copy **Client ID (Consumer Key)** and **Client Secret (Consumer Secret)**
8. Save in Bitwarden

#### Yandex
1. Go to https://oauth.yandex.com/client/new
2. **App name:** `OpenPawz`
3. **Platform:** check **"Web services"**
4. **Callback URI:** `http://127.0.0.1:0/callback`
5. Select scopes as needed
6. Click **"Create"**
7. Copy **Client ID** and **Client Secret**
8. Save in Bitwarden

#### LinkHut
1. Go to https://ln.ht → sign in → account settings → **"OAuth"**
2. **"Register a new OAuth client"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Accounting & Finance

#### QuickBooks / Intuit
1. Go to https://developer.intuit.com/app/developer/dashboard → sign up (free)
2. Click **"Create an app"**
3. Select **"QuickBooks Online and Payments"**
4. **App Name:** `OpenPawz`
5. In your app's **"Keys & credentials"** section:
   - **Redirect URI:** `http://127.0.0.1:0/callback`
6. Copy **Client ID** and **Client Secret** (use the **Development** keys for testing)
7. Save in Bitwarden

> QuickBooks and Intuit share the same developer portal — one registration covers both.

#### Xero
1. Go to https://developer.xero.com/app/manage → **"New app"**
2. **App name:** `OpenPawz`
3. **Integration type:** Web app
4. **Company or application URL:** `https://openpawz.com`
5. **Redirect URI:** `http://127.0.0.1:0/callback`
6. Click **"Create app"**
7. Click **"Generate a secret"** → copy **Client ID** and **Client Secret**
8. Save in Bitwarden

#### Sage
1. Go to https://developer.sage.com → sign up → **"Create App"**
2. **Name:** `OpenPawz`
3. **Callback URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Wave Accounting
1. Go to https://developer.waveapps.com → sign up (free) → **"Create Application"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### FreshBooks
1. Go to https://my.freshbooks.com/#/developer → sign in → **"Create Application"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** (= Application ID) and **Client Secret**
5. Save in Bitwarden

#### Exact Online
1. Go to https://apps.exactonline.com → sign in → **"Manage Apps"** → **"Register"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Mercury
1. Go to https://dashboard.mercury.com/developers → sign in
2. **"Create an Application"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Twinfield
1. Go to https://login.twinfield.com → sign in → navigate to developer settings
2. Create an OAuth application
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Schwab
1. Go to https://developer.schwab.com → sign up (free) → **"Create App"**
2. **App name:** `OpenPawz`
3. **Callback URL:** `http://127.0.0.1:0/callback`
4. Copy **App Key** (= Client ID) and **Secret**
5. Save in Bitwarden

---

### E-Commerce & Payments

#### Stripe
1. Go to https://dashboard.stripe.com/apps → **"Create app"** (or use Stripe Connect OAuth)
2. **App name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Secret Key**
5. Save in Bitwarden

#### PayPal
1. Go to https://developer.paypal.com/developer/applications → log in (free)
2. Under **REST API apps**, click **"Create App"**
3. **App Name:** `OpenPawz`
4. Click **"Create App"** → you're taken to the app details
5. Copy **Client ID** and **Secret** (toggle between Sandbox/Live)
6. For OAuth redirect, go to app settings → add `http://127.0.0.1:0/callback` as Return URL
7. Save in Bitwarden

#### Square
1. Go to https://developer.squareup.com/apps → **"Create Application"**
2. **Application Name:** `OpenPawz`
3. Go to **"OAuth"** tab → add **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Application ID** (= Client ID) and **Application Secret**
5. Save in Bitwarden

#### Mollie
1. Go to https://my.mollie.com/dashboard/developers/applications → **"Create application"**
2. **Name:** `OpenPawz`
3. **Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Braintree
1. Go to https://developer.paypal.com/braintree → sign up (free sandbox) → **"Create App"**
2. **Name:** `OpenPawz`
3. **OAuth Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Amazon (Login with Amazon)
1. Go to https://developer.amazon.com/loginwithamazon → sign in → **"Create a New Security Profile"**
2. **Security Profile Name:** `OpenPawz`
3. **Security Profile Description:** `AI desktop assistant`
4. **Consent Privacy Notice URL:** `https://openpawz.com/privacy`
5. Click **"Save"**
6. Click **"Show Client ID and Client Secret"** or the gear icon → **"Web Settings"**
7. **Allowed Return URLs:** `http://127.0.0.1:0/callback`
8. Copy **Client ID** and **Client Secret**
9. Save in Bitwarden

#### eBay
1. Go to https://developer.ebay.com/my/keys → sign in (free developer program)
2. Click **"Create"** under Application Keys
3. **Application Title:** `OpenPawz`
4. Select **Sandbox** or **Production** environment
5. Under **OAuth Details**, add **RuName** (eBay's redirect URI alias)
6. Set **Accept URL:** `http://127.0.0.1:0/callback`
7. Copy **App ID** (= Client ID) and **Cert ID** (= Client Secret)
8. Save in Bitwarden

#### Printful
1. Go to https://developers.printful.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### ThriveCart
1. Go to https://thrivecart.com/developers → sign in
2. **"Create App"**, **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Ramp
1. Go to https://developer.ramp.com → sign up (free) → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### HR & Recruiting

#### BambooHR
1. Go to https://documentation.bamboohr.com → sign in to your BambooHR instance
2. Go to **Account** → **Apps** → **"Create New Application"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

> BambooHR auth URLs use `{subdomain}.bamboohr.com` — the subdomain is the user's company name.

#### Deel
1. Go to https://developer.deel.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Employment Hero
1. Go to https://developer.employmenthero.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Gusto
1. Go to https://dev.gusto.com → sign up (free) → **"Create Application"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### JobAdder
1. Go to https://developers.jobadder.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Namely
1. Go to https://developers.namely.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

> Auth URL uses `{company}.namely.com` — the company subdomain is instance-specific.

#### Paycor
1. Go to https://developers.paycor.com → sign up (free) → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Payfit
1. Go to https://developers.payfit.io → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Sage People
1. Same as Salesforce — Sage People runs on the Salesforce platform
2. Follow the **Salesforce** walkthrough above
3. Scopes: `offline_access api`
4. Save in Bitwarden

#### Workday
1. Go to https://community.workday.com → sign in → navigate to **"API Clients"**
2. **"Register API Client"**
3. **Name:** `OpenPawz`
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

> Auth URL uses `{domain}/{tenant}` — instance-specific.

#### Zenefits
1. Go to https://developers.zenefits.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### TSheets
1. Go to https://developer.tsheets.com → sign up (free) → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Support & Ticketing

#### Zendesk
1. Go to your Zendesk admin → **Admin Center** → **Apps and integrations** → **APIs** → **Zendesk API**
2. Click **"OAuth Clients"** tab → **"Add OAuth client"**
3. **Client Name:** `OpenPawz`
4. **Redirect URLs:** `http://127.0.0.1:0/callback`
5. Copy **Unique Identifier** (= Client ID) and **Secret**
6. Save in Bitwarden

> Auth URL uses `{subdomain}.zendesk.com` — instance-specific.

#### Intercom
1. Go to https://app.intercom.com/a/apps/_/developer-hub → **"New app"**
2. **App name:** `OpenPawz`
3. In app settings, go to **"Authentication"** → **"OAuth"**
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Help Scout
1. Go to https://developer.helpscout.com → **"My Apps"** → **"Create My App"**
2. **App Name:** `OpenPawz`
3. **Redirection URL:** `http://127.0.0.1:0/callback`
4. Copy **App ID** (= Client ID) and **App Secret**
5. Save in Bitwarden

#### ServiceNow
1. Go to your ServiceNow instance → **System OAuth** → **Application Registry**
2. Click **"New"** → **"Connect to a third party OAuth Provider"** (or create an OAuth Client)
3. **Name:** `OpenPawz`
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

> Auth URL uses `{subdomain}.service-now.com` — instance-specific.

#### NinjaOne RMM
1. Go to https://app.ninjarmm.com → **Administration** → **Apps** → **API**
2. **"Add"** a new client app
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Aircall
1. Go to https://developer.aircall.io → sign in → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Cloud Storage & Files

#### Dropbox
1. Go to https://www.dropbox.com/developers/apps → **"Create app"**
2. Choose **"Scoped access"** → **"Full Dropbox"**
3. **Name:** `OpenPawz` → **"Create app"**
4. Under **OAuth 2 — Redirect URIs**, add `http://127.0.0.1:0/callback`
5. Copy **App key** (= Client ID) and **App secret**
6. Save in Bitwarden

#### Box
1. Go to https://developer.box.com → sign in → **"My Apps"** → **"Create New App"**
2. Choose **"Custom App"** → **"User Authentication (OAuth 2.0)"**
3. **App Name:** `OpenPawz`
4. In **Configuration** tab → **OAuth 2.0 Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### OneDrive Personal
> Uses Microsoft OAuth. Already registered under Microsoft 365.
> If you need a separate registration for personal (consumers) only:
1. Go to Azure Portal → App registrations → New registration
2. **Supported account types:** Personal Microsoft accounts only
3. Redirect URI: **Public client/native** → `http://127.0.0.1:0/callback`
4. Copy **Application (client) ID**
5. Save in Bitwarden

#### Egnyte
1. Go to https://developers.egnyte.com → sign up (free) → **"Register App"**
2. **App Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

> Auth URL uses `{subdomain}.egnyte.com` — instance-specific.

#### Google Drive
> Alias for Google. Already registered under Google Workspace.

#### Contentful
1. Go to https://app.contentful.com/account/profile/developers/applications → **"New Application"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Legal & eSignature

#### DocuSign
1. Go to https://admindemo.docusign.com/apps-and-keys (demo/sandbox) → sign up (free)
2. Click **"Add App and Integration Key"**
3. **App Name:** `OpenPawz`
4. Copy the **Integration Key** (= Client ID)
5. Click **"Add Secret Key"** → copy the secret
6. Under **"Additional settings"**, add **Redirect URI:** `http://127.0.0.1:0/callback`
7. Save in Bitwarden

#### Dropbox Sign (HelloSign)
1. Go to https://app.hellosign.com/home/myAccount#integrations → **"API"** tab
2. Click **"Create App"**
3. **Name:** `OpenPawz`, **OAuth Callback URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Ironclad
1. Go to https://developer.ironcladapp.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### SignNow
1. Go to https://app.signnow.com/api/integrations → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### DATEV
1. Go to https://developer.datev.de → sign up (free) → **"Create App"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Scheduling & Surveys

#### Acuity Scheduling
1. Go to https://acuityscheduling.com/oauth2 → sign in
2. **"Register a new app"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### SurveyMonkey
1. Go to https://developer.surveymonkey.com/apps → **"Create App"**
2. **App name:** `OpenPawz`
3. **OAuth Redirect URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Qualtrics
1. Go to your Qualtrics account → **Account Settings** → **Qualtrics IDs**
2. Under **OAuth**, register a new client
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

> Auth URL uses `{subdomain}.qualtrics.com` — instance-specific.

#### Fillout
1. Go to https://build.fillout.com → sign in → API settings → **"Create OAuth App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Aimfox
1. Go to https://aimfox.com/developers → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Google Workspace
> **Already registered.** One Google OAuth app covers Gmail, Calendar, Drive, Sheets, Docs, YouTube, etc.
> Client ID: `797133120028-...`
> No additional registration needed.

---

### Design & Creative

#### Autodesk
1. Go to https://aps.autodesk.com/myapps → **"Create application"**
2. **App name:** `OpenPawz`, **App description:** `AI desktop assistant`
3. **Callback URL:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### WordPress
1. Go to https://developer.wordpress.com/apps → **"Create New Application"**
2. **Name:** `OpenPawz`
3. **Description:** `AI desktop assistant`
4. **Website URL:** `https://openpawz.com`
5. **Redirect URL:** `http://127.0.0.1:0/callback`
6. Click **"Create"**
7. Copy **Client ID** and **Client Secret**
8. Save in Bitwarden

---

### Analytics & Data

#### Segment
1. Go to https://segment.com → sign in → **Settings** → **Extensions** or **OAuth**
2. Create a new OAuth client
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Addepar
1. Go to https://developers.addepar.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Bitly
1. Go to https://dev.bitly.com → sign in → **"Manage Apps"** → **"Register New App"**
2. **App name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Stack Exchange
1. Go to https://stackapps.com/apps/oauth/register
2. **Application Name:** `OpenPawz`
3. **OAuth Domain:** `127.0.0.1`
4. **Application Website:** `https://openpawz.com`
5. Click **"Register Your Application"**
6. Copy **Client Id** and **Client Secret**
7. Save in Bitwarden

---

### ERP & Operations

#### NetSuite
1. Go to your NetSuite account → **Setup** → **Integration** → **Manage Integrations** → **"New"**
2. **Name:** `OpenPawz`
3. Check **"Token-based Authentication"** and/or **"OAuth 2.0"**
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Consumer Key** (= Client ID) and **Consumer Secret**
6. Save in Bitwarden

> Auth URL uses `{accountId}.app.netsuite.com` — instance-specific.

#### Procore
1. Go to https://developers.procore.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Apaleo
1. Go to https://apaleo.dev → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Bullhorn
1. Go to https://developer.bullhorn.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Odoo
1. Go to your Odoo instance → **Settings** → **Technical** → **OAuth** → **"Create"**
2. **Name:** `OpenPawz`
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

> Auth URL uses `{serverUrl}` — instance-specific.

---

### Communication / Video

#### Zoom
1. Go to https://marketplace.zoom.us/develop/create → choose **"General App"** → **"Create"**
2. **App name:** `OpenPawz`
3. Under **OAuth Information:**
   - **Redirect URL for OAuth:** `http://127.0.0.1:0/callback`
   - **Add allow list:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. In **Scopes**, add: `meeting:read`, `user:read`
6. Save in Bitwarden

#### HeyGen
1. Go to https://app.heygen.com/settings → API / Developer settings
2. Create an OAuth app
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Grain
1. Go to https://grain.com/developers → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Gong
1. Go to https://app.gong.io → **Company Settings** → **API** → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Fathom
1. Go to https://fathom.video/developers → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Ring Central
1. Go to https://developers.ringcentral.com/my-account.html → **"Create App"**
2. **App name:** `OpenPawz`
3. **App type:** Server/Web
4. **OAuth Redirect URI:** `http://127.0.0.1:0/callback`
5. Copy **Client ID** and **Client Secret**
6. Save in Bitwarden

#### Dialpad
1. Go to https://developers.dialpad.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Identity & SSO

#### Okta
1. Go to https://developer.okta.com → sign up (free developer account)
2. In the admin console, go to **Applications** → **"Create App Integration"**
3. **Sign-in method:** OIDC - OpenID Connect
4. **Application type:** Native Application
5. **App integration name:** `OpenPawz`
6. **Sign-in redirect URIs:** `http://127.0.0.1:0/callback`
7. **Assignments:** Skip group assignment for now
8. Click **"Save"**
9. Copy **Client ID** (and **Client Secret** if shown)
10. Save in Bitwarden

> Auth URL uses `{subdomain}.okta.com` — instance-specific.

#### Auth0
1. Go to https://manage.auth0.com → sign up (free) → **"Applications"** → **"Create Application"**
2. **Name:** `OpenPawz`
3. **Application Type:** Native
4. Click **"Create"**
5. In **Settings** tab:
   - **Allowed Callback URLs:** `http://127.0.0.1:0/callback`
6. Copy **Client ID** and **Client Secret** (Domain also shown)
7. Save in Bitwarden

> Auth URL uses `{subdomain}.auth0.com` — instance-specific.

#### PingOne
1. Go to https://docs.pingidentity.com → sign up (free trial) → **"Applications"** → **"+"**
2. **Name:** `OpenPawz`, **Type:** Native
3. **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### ATS / Recruiting

#### Greenhouse Harvest
1. Go to https://developers.greenhouse.io → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Real Estate & Property

#### Reapit
1. Go to https://developers.reapit.cloud → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Wiseagent
1. Go to https://developer.thewiseagent.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Cloudbeds
1. Go to https://developer.cloudbeds.com → sign in → **"Register App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Invoicing & Billing

#### Sellsy
1. Go to https://developers.sellsy.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Teamleader Focus
1. Go to https://developer.teamleader.eu → sign in → **"Create Integration"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### ServiceM8
1. Go to https://developer.servicem8.com → sign up → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Gaming

#### Epic Games
1. Go to https://dev.epicgames.com/portal → sign in → **"Create Application"**
2. **App Name:** `OpenPawz`
3. Under **"Client Credentials"**, add **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Health & Fitness

#### Oura
1. Go to https://cloud.ouraring.com/v2/docs → sign in → **"Create App"** (or Personal Access Tokens)
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Whoop
1. Go to https://developer.whoop.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Health Gorilla
1. Go to https://developer.healthgorilla.com → sign up → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Travel

#### Uber
1. Go to https://developer.uber.com → sign in → **"Create App"**
2. **App name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Construction

#### Hover
1. Go to https://developer.hover.to → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

---

### Adobe Suite

#### Adobe
1. Go to https://developer.adobe.com/console → sign in (free Adobe ID)
2. **"Create new project"** → **"Add API"**
3. Choose the Adobe API you want → Select **"OAuth Server-to-Server"** or **"User Authentication"**
4. **Redirect URI:** `http://127.0.0.1:0/callback`
5. **Application Name:** `OpenPawz`
6. Copy **Client ID** and **Client Secret**
7. Save in Bitwarden

#### Adobe Workfront
1. Go to https://experience.adobe.com → sign in → navigate to **Workfront** → **Setup** → **System** → **OAuth2 Applications**
2. **"Create New"**
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

---

### Additional Services

#### Apollo
1. Go to https://developer.apollo.io → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Blackbaud
1. Go to https://developer.blackbaud.com/apps → **"Register app"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Application ID** (= Client ID) and **Application Secret**
4. Save in Bitwarden

#### Canvas LMS
1. Log in to your Canvas instance as admin
2. Go to **Admin** → **Developer Keys** → **"+ Developer Key"** → **"+ API Key"**
3. **Key Name:** `OpenPawz`, **Redirect URIs:** `http://127.0.0.1:0/callback`
4. Click **"Save"**
5. Copy the **ID** (= Client ID) and **Key** (= Client Secret)
6. Save in Bitwarden

> Auth URL uses `{hostname}` — your Canvas instance URL.

#### Candis
1. Go to https://developer.candis.io → sign in
2. Create an OAuth application
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Kintone
1. Go to your Kintone admin → **System Administration** → **OAuth**
2. **"Register new OAuth client"**
3. **Name:** `OpenPawz`, **Redirect Endpoint:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

> Auth URL uses `{subdomain}.kintone.com` — instance-specific.

#### Maximizer
1. Go to https://developer.maximizer.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### NationBuilder
1. Go to https://nationbuilder.com → sign in → **Settings** → **Developer** → **"Register app"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

> Auth URL uses `{accountId}.nationbuilder.com` — instance-specific.

#### Podium
1. Go to https://developer.podium.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Splitwise
1. Go to https://dev.splitwise.com → sign in → **"Register your application"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. **Homepage URL:** `https://openpawz.com`
4. Copy **Consumer Key** (= Client ID) and **Consumer Secret**
5. Save in Bitwarden

#### Salesmsg
1. Go to https://developer.salesmessage.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Sentry
1. Go to https://sentry.io → sign in → **Settings** → **Developer Settings** → **"Create New Integration"**
2. Choose **"Public Integration"**
3. **Name:** `OpenPawz`
4. **Redirect URL:** `http://127.0.0.1:0/callback`
5. Select permissions as needed
6. Click **"Save Changes"**
7. Copy **Client ID** and **Client Secret**
8. Save in Bitwarden

#### Wildix PBX
1. Go to https://developer.wildix.com → sign in → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

> Auth URL uses `{subdomain}.wildixin.com` — instance-specific.

#### UKG Pro WFM
1. Go to https://developer.ukg.com → sign up (free) → **"Create App"**
2. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
3. Copy **Client ID** and **Client Secret**
4. Save in Bitwarden

#### Adyen
1. Go to https://docs.adyen.com → sign in to your Adyen account → **API Credentials** → **"Create new credential"**
2. Choose **OAuth** credential type
3. **Name:** `OpenPawz`, **Redirect URI:** `http://127.0.0.1:0/callback`
4. Copy **Client ID** and **Client Secret**
5. Save in Bitwarden

#### Meta Marketing (Facebook)
1. Go to https://developers.facebook.com → **"My Apps"** → **"Create App"**
2. **App type:** Business
3. **App name:** `OpenPawz`
4. In app dashboard, go to **"Facebook Login"** → **"Settings"**
5. **Valid OAuth Redirect URIs:** `http://127.0.0.1:0/callback`
6. Click **"Save Changes"**
7. Go to **Settings** → **Basic** → copy **App ID** (= Client ID) and **App Secret**
8. Save in Bitwarden

> App starts in **Development Mode** — works for test users immediately. Submit for App Review later for public access. Not a blocker.

#### AWS Cognito
1. Go to https://console.aws.amazon.com/cognito → sign in to AWS (free tier)
2. Create a **User Pool** (or select existing)
3. Go to **App integration** → **App client settings**
4. Click **"Create app client"**
5. **App client name:** `OpenPawz`
6. **Callback URL(s):** `http://127.0.0.1:0/callback`
7. Under **OAuth 2.0**, select **Authorization code grant** and scopes: `openid`
8. Copy **Client ID** (and **Client Secret** if generated)
9. Save in Bitwarden

> Auth URL uses `{subdomain}.auth.{region}.amazoncognito.com` — instance-specific.

---

## Troubleshooting

### "Port 0 is not valid"
Some portals reject `http://127.0.0.1:0/callback`. Use `http://localhost:19284/callback` instead.

### "Must use HTTPS for redirect URI"
A few portals require HTTPS even for localhost. Try `https://127.0.0.1:0/callback`. If that fails, use `http://localhost:19284/callback` — most portals make an exception for localhost.

### "Need a Company Page / Organization"
Some portals (LinkedIn, Shopify) require a company/org page. Create one for "OpenPawz" — free, takes 2 minutes.

### "App Review Required"
Facebook, Instagram, TikTok require app review for public access. Register the app anyway — you get a Client ID immediately. It works in dev/sandbox mode. Submit for review later.

### "I don't see an OAuth option / only API Keys"
Look for: "OAuth" tab, "Authentication" section, "Connected Apps" (Salesforce), "Authorization" (Atlassian), or a toggle between "API Key" and "OAuth 2.0".

### "Client Secret only shown once"
Some portals only show the secret once when you create it. If you missed it, look for "Regenerate secret" or "New secret". The Client ID stays the same.

### "The portal UI looks different from these instructions"
Portals update their UI frequently. The core process is always the same: find "Create App" or "New Application", fill in name + redirect URI, get Client ID + Secret. The buttons may just be in different places.

---

## Build Integration

After registering, set env vars and build:

```bash
# .env.build (gitignored)
export OPENPAWZ_GITHUB_CLIENT_ID="Ov23li..."
export OPENPAWZ_GOOGLE_CLIENT_ID="797133120028-..."
export OPENPAWZ_DISCORD_CLIENT_ID="1234567890"
# ... add each service as you register it

# Build
source .env.build && cargo tauri build
```

Or for CI/CD (GitHub Actions), add Client IDs as repository variables:
```yaml
env:
  OPENPAWZ_GITHUB_CLIENT_ID: ${{ vars.OPENPAWZ_GITHUB_CLIENT_ID }}
  OPENPAWZ_GOOGLE_CLIENT_ID: ${{ vars.OPENPAWZ_GOOGLE_CLIENT_ID }}
  # ... etc
```
