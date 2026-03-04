# Hybrid OAuth Architecture

> **Status**: Implemented — Tiers 1–3 active, Tier 4 (Nango) deferred
> **Goal**: Zero manual OAuth app registration for end users
> **Strategy**: Tiered broker delegation — PKCE + n8n + RFC 7591 + manual fallback

---

## The Problem

OpenPawz supports 404 integrations. Most require credentials. Our Phase 1–5 OAuth PKCE engine
handles 10 core services, but each requires the *project maintainer* to register an OAuth app
on each platform's developer console. Users who self-host would also need to do this.

We want: **User opens OpenPawz → clicks "Connect GitHub" → done.** No developer console visits.

---

## Honest Assessment of Each Approach

### n8n (Already Embedded)

| What it provides | What it doesn't |
|---|---|
| 101 unique OAuth2 credential types | Pre-registered OAuth apps |
| Full OAuth flow UI at localhost:5678 | Programmatic OAuth trigger from external apps |
| Token refresh, credential encryption | A way to export tokens to other systems |
| 191 nodes with OAuth support | Zero-effort setup — users still register apps in n8n UI |

**Key gap**: `engine_n8n_create_credential` pushes flat key/value data. It does NOT trigger
n8n's interactive OAuth redirect flow. To use n8n's OAuth, users must visit the n8n UI directly.

### Nango (New Addition)

| What it provides | What it doesn't |
|---|---|
| 600+ API provider configs (auth URLs, token refresh, proxy, rate limits) | **Pre-registered OAuth apps** (self-hosted) |
| Connect UI for polished OAuth flows | Zero-effort setup — still need app registration |
| Token auto-refresh and storage | Anything beyond OAuth plumbing |
| Unified API for all providers | Lightweight footprint (needs Postgres + Redis) |

**Critical fact**: Nango self-hosted requires you to register your own OAuth apps.
Only Nango Cloud (SaaS) ships pre-registered apps for 250+ services.
For a self-contained desktop app, self-hosted Nango still requires app registration.

### RFC 7591 Dynamic Client Registration

| What it provides | What it doesn't |
|---|---|
| TRUE zero-registration OAuth | Support from GitHub, Google, Discord, Slack, etc. |
| Auto-register client_id at runtime | Wide adoption (~5 providers support it) |
| Perfect for enterprise OIDC | Consumer API support |

**Supported providers**: Okta, Auth0, Keycloak, some MCP endpoints (Notion MCP, Granola MCP).

### Our PKCE Engine (Already Built)

| What it provides | What it doesn't |
|---|---|
| Direct, fast OAuth for 10 core services | Auto-registration of OAuth apps |
| No external dependencies | Support for 600+ APIs |
| OS keychain storage, auto-refresh | Anything without a Client ID |

---

## Tiered Architecture

The tiers are ordered by **user effort** (lowest first) and **coverage** (broadest first).

```
┌─────────────────────────────────────────────────────────────────┐
│                    User clicks "Connect"                        │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
               ┌───────────────────────┐
               │  Tier Router          │
               │  (check service_id)   │
               └─────┬─────┬─────┬────┘
                     │     │     │
          ┌──────────┘     │     └──────────┐
          ▼                ▼                 ▼
   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
   │ Tier 1       │ │ Tier 2       │ │ Tier 3       │
   │ Ship Client  │ │ n8n OAuth    │ │ RFC 7591     │
   │ IDs in build │ │ Delegation   │ │ Dynamic Reg  │
   │              │ │              │ │              │
   │ 10-15 core   │ │ 101 OAuth2   │ │ ~5 OIDC      │
   │ services     │ │ services via │ │ providers    │
   │              │ │ n8n UI       │ │              │
   │ User: ZERO   │ │ User: visit  │ │ User: ZERO   │
   │ effort       │ │ n8n UI once  │ │ effort       │
   └──────────────┘ └──────────────┘ └──────────────┘
          │                │                 │
          │         ┌──────┘                 │
          │         │    (fallback)          │
          │         ▼                        │
          │  ┌──────────────┐               │
          │  │ Tier 4       │               │
          │  │ Nango Broker │◄──────────────┘
          │  │ (optional)   │   (if Nango installed)
          │  │              │
          │  │ 600+ APIs    │
          │  │ OAuth infra  │
          │  │              │
          │  │ User: reg    │
          │  │ app once     │
          │  └──────────────┘
          │         │
          │         │    (fallback)
          ▼         ▼
   ┌──────────────────────┐
   │ Tier 5               │
   │ Manual API Keys      │
   │ (always available)   │
   │                      │
   │ User: paste key      │
   └──────────────────────┘
```

---

## Tier 1: Shipped Client IDs (ZERO user effort)

**How it works**: OpenPawz project maintainers register OAuth apps on 10–15 core platforms.
Client IDs are embedded in the binary via `option_env!()`. PKCE means no client secret needed.
This is exactly what VS Code, Obsidian, 1Password, and every other desktop app do.

**Services** (already implemented in `oauth.rs`):
- GitHub, Google (Gmail, Sheets, Calendar, Docs, Drive), Discord, Slack
- Notion, Dropbox, Linear, Figma, Reddit, Spotify

**One-time effort**: ~1–2 hours to register 10 apps on their developer consoles.

**Why this matters**: Client IDs are NOT secrets. They're public identifiers. With PKCE,
no client_secret is needed. Every user of OpenPawz shares the same Client IDs.
Their tokens are personal — stored in their OS keychain, never shared.

**Implementation**: ✅ Already complete (Phase 5). Just need to register actual apps.

---

## Tier 2: n8n OAuth Delegation (visit n8n UI once per service)

**How it works**: For services that need n8n workflows, users create OAuth credentials
directly in n8n's web UI. n8n handles the full OAuth dance — redirect, consent, token
exchange, storage, refresh.

**Services**: 101 unique OAuth2 credential types in n8n (GitHub, Google, Slack, HubSpot,
Salesforce, Jira, Stripe, Shopify, Notion, Airtable, Trello, etc.)

**User flow**:
1. User enables an integration in OpenPawz
2. OpenPawz detects it needs n8n credentials
3. Opens n8n credential UI (iframe or redirect to localhost:5678)
4. User completes OAuth in n8n
5. OpenPawz detects credential creation, deploys workflow

**Implementation needed**:
- Detect when a service needs n8n OAuth (check `credential-schemas.json`)
- Open n8n credential creation page for the right credential type
- Poll n8n API for credential creation completion
- Wire credential to workflow deployment

**Key API endpoints**:
```
GET  /api/v1/credential-types         — list available credential types
POST /api/v1/credentials              — create credential (flat data only)
GET  /api/v1/credentials              — list existing credentials
GET  /api/v1/credentials/{id}         — get credential details
```

**Gap**: n8n's REST API creates credentials with flat data. The OAuth redirect flow
is triggered by the n8n web UI, not the API. We need to redirect users to
`http://localhost:5678/credentials/new?type=githubOAuth2Api` and detect completion.

---

## Tier 3: RFC 7591 Dynamic Client Registration (ZERO user effort)

**How it works**: Some OIDC-compliant providers allow clients to self-register at runtime.
The app sends a `POST /register` request with metadata, and receives a `client_id` back.
No human visits a developer console.

**Providers that support this**:
| Provider | Registration Endpoint | Notes |
|---|---|---|
| Okta | `https://{domain}/oauth2/v1/clients` | Enterprise OIDC |
| Auth0 | `https://{domain}/oidc/register` | OIDC Dynamic Registration |
| Keycloak | `https://{domain}/realms/{realm}/clients-registrations/openid-connect` | Self-hosted OIDC |
| Notion MCP | `https://mcp.notion.com/register` | MCP + RFC 7591 |
| Granola MCP | `https://mcp-auth.granola.ai/oauth2/register` | MCP + RFC 7591 |

**Registration payload** (RFC 7591 §2):
```json
{
  "client_name": "OpenPawz",
  "redirect_uris": ["http://127.0.0.1:{port}/callback"],
  "token_endpoint_auth_method": "none",
  "grant_types": ["authorization_code"],
  "response_types": ["code"],
  "application_type": "native"
}
```

**Implementation needed**:
- `dynamic_register_client()` function in `oauth.rs`
- Registration endpoint registry (per-provider)
- Cache registered `client_id` in keychain (no re-registration on re-launch)
- Fall through to normal PKCE flow once `client_id` obtained

---

## Tier 4: Nango Broker (optional power-user addon)

**How it works**: Self-hosted Nango provides OAuth infrastructure for 600+ APIs.
Users register their own OAuth apps in Nango's dashboard, but Nango handles all the
complexity — correct auth URLs, token refresh, proxy with rate limiting, pagination.

**When to use**: Power users who need many integrations beyond the core 10–15.
Nango's value is in its 600+ provider configurations, not in pre-registered apps.

**Infrastructure cost** (3 additional Docker containers):
```yaml
# Nango requires:
paw-nango-db:      postgres:16        # ~50MB RAM
paw-nango-redis:   redis:7.2.4        # ~10MB RAM
paw-nango-server:  nangohq/nango-server:hosted  # ~200MB RAM
```

**Total added overhead**: ~260MB RAM, ~1.5GB disk (images)

**User flow**:
1. User enables "Nango Power Mode" in settings
2. OpenPawz provisions 3 Nango Docker containers alongside n8n
3. User visits Nango dashboard at localhost:3003 to configure OAuth apps
4. OpenPawz uses Nango's Connect Sessions API to trigger OAuth flows
5. Tokens stored in Nango, proxied by OpenPawz

**Connect Sessions API**:
```
POST /connect/sessions
Authorization: Bearer <nango-secret-key>
{
  "end_user": { "id": "openpawz-user" },
  "allowed_integrations": ["github", "slack", "notion"]
}
→ { "data": { "token": "...", "connect_link": "http://localhost:3009/..." } }
```

**Implementation**: DEFERRED — Tier 4 is optional. Focus on Tiers 1–3 first.

---

## Service Routing Table

How the tier router decides which tier handles a service:

```rust
fn resolve_tier(service_id: &str) -> OAuthTier {
    // Tier 1: Services with shipped Client IDs
    if SHIPPED_OAUTH_CONFIGS.contains_key(service_id) {
        return OAuthTier::ShippedPkce;
    }

    // Tier 3: Services with RFC 7591 dynamic registration
    if RFC7591_REGISTRY.contains_key(service_id) {
        return OAuthTier::DynamicRegistration;
    }

    // Tier 2: Services with n8n OAuth credential types
    if N8N_OAUTH_TYPES.contains_key(service_id) {
        return OAuthTier::N8nDelegation;
    }

    // Tier 4: Services in Nango catalog (if installed)
    if nango_installed() && NANGO_PROVIDERS.contains_key(service_id) {
        return OAuthTier::NangoBroker;
    }

    // Tier 5: Manual API key
    OAuthTier::ManualApiKey
}
```

---

## Implementation Priority

### Phase A — Ship Client IDs (Tier 1) ⏱️ 1-2 hours
Register 10 OAuth apps. Set env vars. Build and ship.
This alone covers the most-used services with ZERO user effort.

### Phase B — n8n OAuth Delegation (Tier 2) ⏱️ 4-6 hours
- Map 101 n8n OAuth2 credential types to OpenPawz service IDs
- Build credential creation redirect (open n8n UI for the right type)
- Poll for credential completion
- Wire to workflow deployment

### Phase C — RFC 7591 Dynamic Registration (Tier 3) ⏱️ 2-3 hours
- Implement `dynamic_register_client()` in oauth.rs
- Add 5 provider registration endpoints
- Cache client_id in keychain
- Chain into existing PKCE flow

### Phase D — Nango Broker (Tier 4) ⏱️ 6-8 hours
- Docker provisioning for 3 Nango containers
- Nango API client for Connect Sessions
- Provider mapping (service_id → nango integration slug)
- Settings UI for enabling/disabling Nango

---

## Coverage Summary

| Tier | Services | User Effort | Status |
|------|----------|-------------|--------|
| 1 — Shipped IDs | 10–15 core | ZERO | ✅ Code done, need app registration |
| 2 — n8n OAuth | ~101 OAuth types | Visit n8n UI once | 🔲 Needs delegation bridge |
| 3 — RFC 7591 | ~5 OIDC providers | ZERO | 🔲 Needs implementation |
| 4 — Nango (opt.) | 600+ APIs | Register in Nango | 🔲 Deferred |
| 5 — API Keys | All 404 services | Paste key | ✅ Already working |

**Combined Tier 1 + 2 + 3 coverage**: ~116 services with minimal-to-zero user effort.
**With Tier 5 fallback**: All 404 services covered.

---

## Architecture Decisions

### Why NOT Nango Cloud?
- Nango Cloud provides pre-registered OAuth apps (250+) but:
  - Requires internet connectivity to Nango's SaaS
  - Adds a third-party dependency to user data flow
  - Not self-contained (violates OpenPawz's offline-first principle)
  - Pricing for self-hosted: free tier exists, but limits apply

### Why Tier 4 (Nango self-hosted) is optional?
- Adds 3 Docker containers (~260MB RAM)
- Still requires OAuth app registration (same as direct PKCE)
- Main value: provider configs for 600+ APIs and token management
- Only needed for power users wanting many integrations

### Why ship Client IDs in the binary?
- Client IDs are NOT secrets — they're public identifiers
- PKCE (RFC 7636) eliminates the need for client_secret
- This is industry standard: VS Code, Obsidian, Slack desktop, 1Password all do it
- One-time registration by project maintainers benefits ALL users

### Why n8n delegation uses redirect, not API?
- n8n's REST API creates credentials with flat data
- OAuth requires browser redirect for user consent
- n8n's web UI handles the full OAuth dance internally
- We redirect users to the n8n credential creation page and detect completion
