# OAuth App Registration Checklist

> **Total: 174 services** listed below.
> **247 provider endpoint configs** available in `providers.json` (from Nango OSS, MIT-licensed).
> Aliases (YouTube = Google, Outlook = Microsoft, etc.) share one registration — no duplicate work.

## Registration Instructions

**Every developer portal listed below is free.** You are registering an OAuth *app* (a client ID), not signing up for the paid product. The developer portal is always separate from the product's pricing. Do not skip any service.

**For step-by-step instructions on each service, see the [Registration Guide](oauth-app-registration.md).** It tells you exactly what to click on every portal.

### What You Need

| Field | Value |
|-------|-------|
| **App name** | `OpenPawz` |
| **App type** | Native / Desktop / Installed (not "web server") |
| **Website** | `https://openpawz.com` |
| **Redirect URI** | `http://127.0.0.1:0/callback` (if the portal rejects port 0, use `http://localhost:19284/callback`) |
| **Email** | `dev@openpawz.ai` |

### Steps (Same for Every Service)

1. Click the **Developer Console** link in the table below
2. Sign up or log in (free — use `dev@openpawz.ai`)
3. Create a new OAuth2 app with the values above
4. Copy the **Client ID** (and **Client Secret** if one is issued)
5. Save both in your Bitwarden entry for that service
6. Mark the row ✅ in this checklist

### Platform-Specific Notes

A few platforms have an extra step beyond basic registration. These are **not blockers** — you can still register the app and get a client ID. The extra step only matters for production/public use later.

| Platform | Extra Step | Impact |
|----------|-----------|--------|
| **Facebook / Instagram / Meta Marketing** | App Review required for production | App works in Development Mode immediately — limited to test users. Submit for review later. |
| **TikTok** | App Review required for production | Same as Facebook — dev mode works now, review later. |
| **Twitter/X** | Developer access application | Usually approved same-day. Apply at developer.twitter.com. |
| **Shopify** | Requires free Partners account | Sign up at partners.shopify.com (free). Then create the app from there. |

**Status Legend:** ⬜ Not started | 🔄 In progress | ✅ Registered

---

## ✅ Currently Working

| Service | Client ID | Tier | Config |
|---|---|---|---|
| ✅ **Google Workspace** | `797133120028-...` | 1a static | `oauth.rs GOOGLE_OAUTH` |
| ✅ **Microsoft 365** | `e1026883-ecd3-...` | 1a static | `oauth.rs MICROSOFT_OAUTH` |

---

## Productivity & Project Management

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 1 | **Asana** | https://app.asana.com/0/developer-console | `https://app.asana.com/-/oauth_authorize` | `default` | ✓ | ⬜ |
| 2 | **Basecamp** | https://launchpad.37signals.com/integrations | `https://launchpad.37signals.com/authorization/new` | — | ✓ | ⬜ |
| 3 | **ClickUp** | https://app.clickup.com/settings/integrations | `https://app.clickup.com/api` | — | ✓ | ⬜ |
| 4 | **Figma** | https://www.figma.com/developers/apps | `https://www.figma.com/oauth` | — | ✗ | ⬜ |
| 5 | **Harvest** | https://id.getharvest.com/oauth2/access_tokens | `https://id.getharvest.com/oauth2/authorize` | — | ✓ | ⬜ |
| 6 | **Linear** | https://linear.app/settings/api | `https://linear.app/oauth/authorize` | — | ✗ | ⬜ |
| 7 | **Miro** | https://developers.miro.com/page/get-started | `https://miro.com/oauth/authorize` | — | ✓ | ⬜ |
| 8 | **Monday.com** | https://monday.com/developers/apps | `https://auth.monday.com/oauth2/authorize` | — | ✓ | ⬜ |
| 9 | **ProductBoard** | https://developer.productboard.com | `https://app.productboard.com/oauth2/authorize` | — | ✓ | ⬜ |
| 10 | **Slack** | https://api.slack.com/apps | `https://slack.com/oauth/v2/authorize` | — | ✗ | ⬜ |
| 11 | **Teamwork** | https://developer.teamwork.com | `https://www.teamwork.com/launchpad/login` | — | ✓ | ⬜ |
| 12 | **TickTick** | https://developer.ticktick.com/manage | `https://ticktick.com/oauth/authorize` | — | ✓ | ⬜ |
| 13 | **Timely** | https://timelyapp.com/developer | `https://api.timelyapp.com/1.1/oauth/authorize` | — | ✗ | ⬜ |
| 14 | **Wrike** | https://www.wrike.com/apps/api | `https://login.wrike.com/oauth2/authorize/v4` | — | ✓ | ⬜ |
| 15 | **Canva** | https://www.canva.com/developers/ | `https://www.canva.com/api/oauth/authorize` | — | ✓ | ⬜ |
| 16 | **Mural** | https://developers.mural.co | `https://app.mural.co/api/public/v1/authorization/oauth2` | — | ✓ | ⬜ |
| 17 | **Envoy** | https://developers.envoy.com | `https://app.envoy.com/a/auth/v0/authorize` | — | ✓ | ⬜ |
| 18 | **Workable** | https://developer.workable.com | `https://www.workable.com/oauth/authorize` | — | ✓ | ⬜ |

## CRM & Sales

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 19 | **HubSpot** | https://developers.hubspot.com | `https://app.hubspot.com/oauth/authorize` | — | ✓ | ⬜ |
| 20 | **Salesforce** | https://developer.salesforce.com | `https://login.salesforce.com/services/oauth2/authorize` | `offline_access` | ✓ | ⬜ |
| 21 | **Pipedrive** | https://developers.pipedrive.com | `https://oauth.pipedrive.com/oauth/authorize` | — | ✗ | ⬜ |
| 22 | **Close** | https://developer.close.com | `https://app.close.com/oauth2/authorize` | `offline_access` | ✓ | ⬜ |
| 23 | **Copper** | https://developer.copper.com | `https://app.copper.com/oauth/authorize` | `developer/v1/all` | ✓ | ⬜ |
| 24 | **Attio** | https://developers.attio.com | `https://app.attio.com/authorize` | — | ✓ | ⬜ |
| 25 | **Zoho** | https://api-console.zoho.com | `https://accounts.zoho.com/oauth/v2/auth` | — | ✓ | ⬜ |
| 26 | **Zendesk Sell** | https://developer.zendesk.com | `https://api.getbase.com/oauth2/authorize` | — | ✓ | ⬜ |
| 27 | **Wealthbox** | https://dev.wealthbox.com | `https://app.crmworkspace.com/oauth/authorize` | — | ✓ | ⬜ |
| 28 | **PreciseFP** | https://developer.precisefp.com | `https://app.precisefp.com/oauth/authorize` | `*` | ✓ | ⬜ |

## Communication & Social

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 29 | **Discord** | https://discord.com/developers/applications | `https://discord.com/api/oauth2/authorize` | — | ✓ | ⬜ |
| 30 | **Microsoft** | https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps | `https://login.microsoftonline.com/common/oauth2/v2.0/authorize` | `offline_access .default` | ✗ | ⬜ |
| 31 | **Webex** | https://developer.webex.com/my-apps | `https://webexapis.com/v1/authorize` | — | ✓ | ⬜ |
| 32 | **Tumblr** | https://www.tumblr.com/oauth/apps | `https://www.tumblr.com/oauth2/authorize` | — | ✓ | ⬜ |
| 33 | **Reddit** | https://www.reddit.com/prefs/apps | `https://www.reddit.com/api/v1/authorize` | `permanent` | ✓ | ⬜ |

## Developer Tools & DevOps

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 34 | **GitHub** | https://github.com/settings/developers | `https://github.com/login/oauth/authorize` | — | ✓ | ⬜ |
| 35 | **Bitbucket** | https://bitbucket.org/account/settings/app-authorizations/ | `https://bitbucket.org/site/oauth2/authorize` | — | ✓ | ⬜ |
| 36 | **Atlassian/Jira** | https://developer.atlassian.com/console/myapps/ | `https://auth.atlassian.com/authorize` | `offline_access` | ✓ | ⬜ |
| 37 | **DigitalOcean** | https://cloud.digitalocean.com/account/api/applications | `https://cloud.digitalocean.com/v1/oauth/authorize` | — | ✓ | ⬜ |
| 38 | **PagerDuty** | https://developer.pagerduty.com/apps | `https://app.pagerduty.com/oauth/authorize` | — | ✓ | ⬜ |
| 39 | **Webflow** | https://developers.webflow.com | `https://webflow.com/oauth/authorize` | — | ✓ | ⬜ |
| 40 | **Zapier** | https://developer.zapier.com | `https://api.zapier.com/v2/authorize` | — | ✗ | ⬜ |
| 41 | **WakaTime** | https://wakatime.com/apps | `https://wakatime.com/oauth/authorize` | — | ✓ | ⬜ |
| 42 | **Snowflake** | https://docs.snowflake.com/en/user-guide/oauth-custom | `https://{account}.snowflakecomputing.com/oauth/authorize` | — | ✓ | ⬜ |
| 43 | **Squarespace** | https://developers.squarespace.com | `https://login.squarespace.com/api/1/login/oauth/provider/authorize` | — | ✓ | ⬜ |

## Marketing & Email

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 44 | **Mailchimp** | https://admin.mailchimp.com/account/oauth2/ | `https://login.mailchimp.com/oauth2/authorize` | — | ✓ | ⬜ |
| 45 | **Constant Contact** | https://app.constantcontact.com/pages/dma/portal/ | `https://authz.constantcontact.com/oauth2/default/v1/authorize` | `offline_access` | ✗ | ⬜ |
| 46 | **Outreach** | https://developers.outreach.io | `https://api.outreach.io/oauth/authorize` | — | ✓ | ⬜ |
| 47 | **SalesLoft** | https://developers.salesloft.com | `https://accounts.salesloft.com/oauth/authorize` | — | ✓ | ⬜ |
| 48 | **Keap (Infusionsoft)** | https://developer.keap.com | `https://accounts.infusionsoft.com/app/oauth/authorize` | — | ✓ | ⬜ |
| 49 | **HighLevel** | https://marketplace.gohighlevel.com | `https://marketplace.gohighlevel.com/oauth/chooselocation` | — | ✗ | ⬜ |
| 50 | **Brex** | https://developer.brex.com | `https://accounts-api.brex.com/oauth2/default/v1/authorize` | — | ✓ | ⬜ |

## Social Media & Video

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 51 | **Twitter/X (v2)** | https://developer.twitter.com/en/portal | `https://twitter.com/i/oauth2/authorize` | `offline.access` | ✓ | ⬜ |
| 52 | **LinkedIn** | https://www.linkedin.com/developers/apps | `https://www.linkedin.com/oauth/v2/authorization` | — | ✗ | ⬜ |
| 53 | **TikTok Accounts** | https://developers.tiktok.com | `https://www.tiktok.com/v2/auth/authorize/` | — | ✓ | ⬜ |
| 54 | **TikTok Ads** | https://business.tiktok.com/apps | `https://business-api.tiktok.com/portal/auth` | — | ✓ | ⬜ |
| 55 | **TikTok Personal** | https://developers.tiktok.com | `https://www.tiktok.com/v2/auth/authorize/` | — | ✓ | ⬜ |
| 56 | **Snapchat** | https://business.snapchat.com/developer | `https://accounts.snapchat.com/login/oauth2/authorize` | — | ✗ | ⬜ |
| 57 | **Pinterest** | https://developers.pinterest.com | `https://www.pinterest.com/oauth` | — | ✓ | ⬜ |
| 58 | **Spotify** | https://developer.spotify.com/dashboard | `https://accounts.spotify.com/authorize` | — | ✓ | ⬜ |
| 59 | **Twitch** | https://dev.twitch.tv/console/apps | `https://id.twitch.tv/oauth2/authorize` | — | ✓ | ⬜ |
| 60 | **Vimeo** | https://developer.vimeo.com/apps | `https://api.vimeo.com/oauth/authorize` | — | ✓ | ⬜ |
| 61 | **YouTube** | https://console.cloud.google.com/apis | _(alias: Google OAuth)_ | — | ✓ | ⬜ |
| 62 | **Strava** | https://www.strava.com/settings/api | `https://www.strava.com/oauth/authorize` | — | ✓ | ⬜ |
| 63 | **Osu** | https://osu.ppy.sh/home/account/edit#oauth | `https://osu.ppy.sh/oauth/authorize` | `identify` | ✓ | ⬜ |
| 64 | **Yahoo** | https://developer.yahoo.com/apps | `https://api.login.yahoo.com/oauth2/request_auth` | — | ✓ | ⬜ |
| 65 | **Yandex** | https://oauth.yandex.com/client/new | `https://oauth.yandex.com/authorize` | — | ✓ | ⬜ |
| 66 | **LinkHut** | https://ln.ht | `https://ln.ht/_/oauth/authorize` | — | ✓ | ⬜ |

## Accounting & Finance

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 67 | **QuickBooks** | https://developer.intuit.com/app/developer/dashboard | `https://appcenter.intuit.com/connect/oauth2` | — | ✓ | ⬜ |
| 68 | **Intuit** | https://developer.intuit.com | `https://appcenter.intuit.com/connect/oauth2` | — | ✓ | ⬜ |
| 69 | **Xero** | https://developer.xero.com/app/manage | `https://login.xero.com/identity/connect/authorize` | `offline_access` | ✓ | ⬜ |
| 70 | **Sage** | https://developer.sage.com | `https://www.sageone.com/oauth2/auth/central` | — | ✓ | ⬜ |
| 71 | **Wave Accounting** | https://developer.waveapps.com | `https://api.waveapps.com/oauth2/authorize` | — | ✓ | ⬜ |
| 72 | **FreshBooks** | https://my.freshbooks.com/#/developer | `https://auth.freshbooks.com/oauth/authorize` | — | ✓ | ⬜ |
| 73 | **Exact Online** | https://apps.exactonline.com | `https://start.exactonline.{ext}/api/oauth2/auth` | — | ✓ | ⬜ |
| 74 | **Mercury** | https://dashboard.mercury.com/developers | `https://oauth2.mercury.com/oauth2/auth` | `offline_access` | ✓ | ⬜ |
| 75 | **Twinfield** | https://login.twinfield.com | `https://login.twinfield.com/auth/authentication/connect/authorize` | `openid twf.user offline_access` | ✓ | ⬜ |
| 76 | **Schwab** | https://developer.schwab.com | `https://api.schwabapi.com/v1/oauth/authorize` | — | ✗ | ⬜ |

## E-Commerce & Payments

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 77 | **Stripe App** | https://dashboard.stripe.com/apps | `https://marketplace.stripe.com/oauth/v2/authorize` | — | ✗ | ⬜ |
| 78 | **PayPal** | https://developer.paypal.com/developer/applications | `https://www.paypal.com/signin/authorize` | — | ✓ | ⬜ |
| 79 | **Square** | https://developer.squareup.com/apps | `https://connect.squareup.com/oauth2/authorize` | — | ✗ | ⬜ |
| 80 | **Mollie** | https://my.mollie.com/dashboard/developers/applications | `https://my.mollie.com/oauth2/authorize` | — | ✗ | ⬜ |
| 81 | **Braintree** | https://developer.paypal.com/braintree | `https://api.braintreegateway.com/oauth/connect` | — | ✓ | ⬜ |
| 82 | **Amazon** | https://developer.amazon.com/loginwithamazon | `https://www.amazon.com/ap/oa` | — | ✓ | ⬜ |
| 83 | **eBay** | https://developer.ebay.com/my/keys | `https://auth.ebay.com/oauth2/authorize` | — | ✓ | ⬜ |
| 84 | **Printful** | https://developers.printful.com | `https://www.printful.com/oauth/authorize` | — | ✗ | ⬜ |
| 85 | **ThriveCart** | https://thrivecart.com/developers | `https://thrivecart.com/authorization/new` | — | ✓ | ⬜ |
| 86 | **Ramp** | https://developer.ramp.com | `https://app.ramp.com/v1/authorize` | — | ✓ | ⬜ |

## HR & Recruiting

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 87 | **BambooHR** | https://documentation.bamboohr.com | `https://{subdomain}.bamboohr.com/authorize.php` | — | ✓ | ⬜ |
| 88 | **Deel** | https://developer.deel.com | `https://app.deel.com/oauth2/authorize` | — | ✓ | ⬜ |
| 89 | **Employment Hero** | https://developer.employmenthero.com | `https://oauth.employmenthero.com/oauth2/authorize` | — | ✗ | ⬜ |
| 90 | **Gusto** | https://dev.gusto.com | `https://api.gusto.com/oauth/authorize` | — | ✓ | ⬜ |
| 91 | **JobAdder** | https://developers.jobadder.com | `https://id.jobadder.com/connect/authorize` | `offline_access` | ✓ | ⬜ |
| 92 | **Namely** | https://developers.namely.com | `https://{company}.namely.com/api/v1/oauth2/authorize` | — | ✓ | ⬜ |
| 93 | **Paycor** | https://developers.paycor.com | `https://hcm.paycor.com/AppActivation/Authorize` | `offline_access` | ✓ | ⬜ |
| 94 | **Payfit** | https://developers.payfit.io | `https://oauth.payfit.com/authorize` | — | ✓ | ⬜ |
| 95 | **Sage People** | https://developer.salesforce.com | `https://login.salesforce.com/services/oauth2/authorize` | `offline_access api` | ✓ | ⬜ |
| 96 | **Workday** | https://community.workday.com | `https://{domain}/{tenant}/authorize` | — | ✓ | ⬜ |
| 97 | **Zenefits** | https://developers.zenefits.com | `https://secure.zenefits.com/oauth2/platform-authorize` | — | ✓ | ⬜ |
| 98 | **TSheets** | https://developer.tsheets.com | `https://rest.tsheets.com/api/v1/authorize` | — | ✓ | ⬜ |

## Support & Ticketing

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 99 | **Zendesk** | https://developer.zendesk.com/api-reference | `https://{subdomain}.zendesk.com/oauth/authorizations/new` | — | ✓ | ⬜ |
| 100 | **Intercom** | https://app.intercom.com/a/apps/_/developer-hub | `https://app.intercom.com/oauth` | — | ✓ | ⬜ |
| 101 | **Help Scout** | https://developer.helpscout.com | `https://secure.helpscout.net/authentication/authorizeClientApplication` | — | ✓ | ⬜ |
| 102 | **ServiceNow** | https://developer.servicenow.com | `https://{subdomain}.service-now.com/oauth_auth.do` | — | ✓ | ⬜ |
| 103 | **NinjaOne RMM** | https://app.ninjarmm.com | `https://app.ninjarmm.com/ws/oauth/authorize` | `offline_access` | ✓ | ⬜ |
| 104 | **Aircall** | https://developer.aircall.io | `https://dashboard.aircall.io/oauth/authorize` | — | ✓ | ⬜ |

## Cloud Storage & Files

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 105 | **Dropbox** | https://www.dropbox.com/developers/apps | `https://www.dropbox.com/oauth2/authorize` | — | ✓ | ⬜ |
| 106 | **Box** | https://developer.box.com/guides/applications/ | `https://account.box.com/api/oauth2/authorize` | — | ✓ | ⬜ |
| 107 | **OneDrive Personal** | https://portal.azure.com | `https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize` | `offline_access` | ✗ | ⬜ |
| 108 | **Egnyte** | https://developers.egnyte.com | `https://{subdomain}.egnyte.com/puboauth/token` | — | ✓ | ⬜ |
| 109 | **Google Drive** | https://console.cloud.google.com/apis | _(alias: Google OAuth)_ | — | ✓ | ⬜ |
| 110 | **Contentful** | https://app.contentful.com/account/profile/developers/applications | `https://be.contentful.com/oauth/authorize` | — | ✓ | ⬜ |

## Legal & eSignature

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 111 | **DocuSign** | https://admindemo.docusign.com/apps-and-keys | `https://account.docusign.com/oauth/auth` | — | ✓ | ⬜ |
| 112 | **Dropbox Sign (HelloSign)** | https://app.hellosign.com/home/myAccount#integrations | `https://app.hellosign.com/oauth/authorize` | — | ✓ | ⬜ |
| 113 | **Ironclad** | https://developer.ironcladapp.com | `https://ironcladapp.com/oauth/authorize` | — | ✗ | ⬜ |
| 114 | **SignNow** | https://app.signnow.com/api/integrations | `https://app.signnow.com/authorize` | — | ✗ | ⬜ |
| 115 | **DATEV** | https://developer.datev.de | `https://login.datev.de/openid/authorize` | `openid` | ✓ | ⬜ |

## Scheduling & Surveys

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 116 | **Acuity Scheduling** | https://acuityscheduling.com/oauth2 | `https://acuityscheduling.com/oauth2/authorize` | `api-v1` | ✓ | ⬜ |
| 117 | **SurveyMonkey** | https://developer.surveymonkey.com/apps | `https://api.surveymonkey.com/oauth/authorize` | — | ✗ | ⬜ |
| 118 | **Qualtrics** | https://developer.qualtrics.com | `https://{subdomain}.qualtrics.com/oauth2/auth` | — | ✓ | ⬜ |
| 119 | **Fillout** | https://build.fillout.com | `https://build.fillout.com/authorize/oauth` | — | ✓ | ⬜ |
| 120 | **Aimfox** | https://aimfox.com/developers | `https://id.aimfox.com/realms/aimfox-prod/protocol/openid-connect/auth` | — | ✗ | ⬜ |

## Google Workspace (Single Registration)

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 121 | **Google (all services)** | https://console.cloud.google.com/apis/credentials | `https://accounts.google.com/o/oauth2/auth` | `offline_access` + per-API scopes | ✓ | ⬜ |

> One Google OAuth app covers: Gmail, Calendar, Drive, Sheets, Docs, YouTube, Cloud Storage, Workspace Admin, Google Play, etc.

## Design & Creative

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 122 | **Autodesk** | https://aps.autodesk.com/myapps | `https://developer.api.autodesk.com/authentication/v2/authorize` | — | ✗ | ⬜ |
| 123 | **WordPress** | https://developer.wordpress.com/apps | `https://public-api.wordpress.com/oauth2/authorize` | — | ✓ | ⬜ |

## Analytics & Data

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 124 | **Segment** | https://segment.com/docs/connections | `https://id.segmentapis.com/oauth2/auth` | — | ✓ | ⬜ |
| 125 | **Addepar** | https://developers.addepar.com | `https://id.addepar.com/oauth2/authorize` | — | ✗ | ⬜ |
| 126 | **Bitly** | https://dev.bitly.com | `https://bitly.com/oauth/authorize` | — | ✓ | ⬜ |
| 127 | **Strava** | _(see Social/Sports)_ | — | — | — | — |
| 128 | **Stack Exchange** | https://stackapps.com/apps/oauth/register | `https://stackoverflow.com/oauth` | `no_expiry` | ✓ | ⬜ |

## ERP & Operations

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 129 | **NetSuite** | https://system.netsuite.com | `https://{accountId}.app.netsuite.com/app/login/oauth2/authorize.nl` | `rest_webservices` | ✓ | ⬜ |
| 130 | **Procore** | https://developers.procore.com/documentation/building-apps | `https://login.procore.com/oauth/authorize` | — | ✓ | ⬜ |
| 131 | **Apaleo** | https://apaleo.dev | `https://identity.apaleo.com/connect/authorize` | — | ✓ | ⬜ |
| 132 | **Bullhorn** | https://developer.bullhorn.com | `https://auth-west.bullhornstaffing.com/oauth/authorize` | — | ✗ | ⬜ |
| 133 | **Odoo** | https://www.odoo.com/documentation/developer | `https://{serverUrl}/restapi/1.0/common/oauth2/authorize` | — | ✓ | ⬜ |

## Communication / Video

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 134 | **Zoom** | https://marketplace.zoom.us/develop/create | `https://zoom.us/oauth/authorize` | — | ✓ | ⬜ |
| 135 | **HeyGen** | https://app.heygen.com/settings | `https://app.heygen.com/oauth/authorize` | — | ✓ | ⬜ |
| 136 | **Grain** | https://grain.com/developers | `https://grain.com/_/public-api/oauth2/authorize` | — | ✓ | ⬜ |
| 137 | **Gong** | https://app.gong.io/company/api-authentication | `https://app.gong.io/oauth2/authorize` | — | ✗ | ⬜ |
| 138 | **Fathom** | https://fathom.video/developers | `https://fathom.video/external/v1/oauth2/authorize` | — | ✗ | ⬜ |
| 139 | **Ring Central** | https://developers.ringcentral.com/my-account.html | `https://platform.ringcentral.com/restapi/oauth/authorize` | — | ✓ | ⬜ |
| 140 | **Dialpad** | https://developers.dialpad.com | `https://dialpad.com/oauth2/authorize` | — | ✓ | ⬜ |

## Identity & SSO

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 141 | **Okta** | https://developer.okta.com | `https://{subdomain}.okta.com/oauth2/v1/authorize` | — | ✓ | ⬜ |
| 142 | **Auth0** | https://manage.auth0.com | `https://{subdomain}.auth0.com/authorize` | — | ✓ | ⬜ |
| 143 | **PingOne** | https://docs.pingidentity.com | `https://auth.pingone.{tld}/{envId}/as/authorize` | — | ✓ | ⬜ |

## ATS / Greenhouse

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 144 | **Greenhouse Harvest** | https://developers.greenhouse.io | `https://app.greenhouse.io/oauth/authorize` | — | ✓ | ⬜ |

## Real Estate & Property

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 145 | **Reapit** | https://developers.reapit.cloud | `https://connect.reapit.cloud/authorize` | — | ✗ | ⬜ |
| 146 | **Wiseagent** | https://developer.thewiseagent.com | `https://sync.thewiseagent.com/WiseAuth/auth` | — | ✗ | ⬜ |
| 147 | **Cloudbeds** | https://developer.cloudbeds.com | `https://hotels.cloudbeds.com/api/v1.3/oauth` | — | ✗ | ⬜ |

## Invoicing & Billing

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 148 | **Sellsy** | https://developers.sellsy.com | `https://login.sellsy.com/oauth2/authorization` | — | ✓ | ⬜ |
| 149 | **Teamleader Focus** | https://developer.teamleader.eu | `https://focus.teamleader.eu/oauth2/authorize` | — | ✓ | ⬜ |
| 150 | **ServiceM8** | https://developer.servicem8.com | `https://go.servicem8.com/oauth/authorize` | — | ✓ | ⬜ |

## Gaming

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 151 | **Epic Games** | https://dev.epicgames.com/portal | `https://www.epicgames.com/id/authorize` | — | ✓ | ⬜ |

## Health & Fitness

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 152 | **Oura** | https://cloud.ouraring.com/v2/docs | `https://cloud.ouraring.com/oauth/authorize` | — | ✓ | ⬜ |
| 153 | **Whoop** | https://developer.whoop.com | `https://api.prod.whoop.com/oauth/oauth2/auth` | — | ✓ | ⬜ |
| 154 | **Health Gorilla** | https://developer.healthgorilla.com | `https://api.healthgorilla.com/oauth/authorize` | — | ✓ | ⬜ |

## Travel & Hospitality

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 155 | **Uber** | https://developer.uber.com | `https://login.uber.com/oauth/v2/authorize` | — | ✓ | ⬜ |

## Construction

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 156 | **Hover** | https://developer.hover.to | `https://hover.to/oauth/authorize` | — | ✗ | ⬜ |

## Adobe Suite

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 157 | **Adobe** | https://developer.adobe.com/console | `https://ims-na1.adobelogin.com/ims/authorize/v2` | `offline_access` | ✓ | ⬜ |
| 158 | **Adobe Workfront** | https://experience.adobe.com | `https://{hostname}/integrations/oauth2/authorize` | — | ✓ | ⬜ |

## Additional Notable Services

| # | Service | Developer Console | Auth URL | Scopes | PKCE | Status |
|---|---------|------------------|----------|--------|------|--------|
| 159 | **Apollo** | https://developer.apollo.io | `https://app.apollo.io/oauth/authorize` | — | ✗ | ⬜ |
| 160 | **Blackbaud** | https://developer.blackbaud.com/apps | `https://app.blackbaud.com/oauth/authorize` | — | ✓ | ⬜ |
| 161 | **Canvas LMS** | https://canvas.instructure.com/doc/api | `https://{hostname}/login/oauth2/auth` | — | ✓ | ⬜ |
| 162 | **Candis** | https://developer.candis.io | `https://id.my.candis.io/auth/realms/candis/...` | — | ✗ | ⬜ |
| 163 | **Kintone** | https://developer.kintone.com | `https://{subdomain}.kintone.com/oauth2/authorization` | — | ✓ | ⬜ |
| 164 | **Maximizer** | https://developer.maximizer.com | `https://{region}.maximizercrmlive.com/oauth2/{alias}/authorize` | — | ✗ | ⬜ |
| 165 | **NationBuilder** | https://nationbuilder.com/api | `https://{accountId}.nationbuilder.com/oauth/authorize` | `default` | ✓ | ⬜ |
| 166 | **Podium** | https://developer.podium.com | `https://api.podium.com/oauth/authorize` | — | ✓ | ⬜ |
| 167 | **Splitwise** | https://dev.splitwise.com | `https://secure.splitwise.com/oauth/authorize` | — | ✓ | ⬜ |
| 168 | **Salesmsg** | https://developer.salesmessage.com | `https://app.salesmessage.com/auth/oauth` | — | ✗ | ⬜ |
| 169 | **Sentry** | https://sentry.io/settings/developer-settings/ | `https://sentry.io/oauth/authorize/` | — | ✓ | ⬜ |
| 170 | **Wildix PBX** | https://developer.wildix.com | `https://{subdomain}.wildixin.com/authorization/oauth2` | — | ✓ | ⬜ |
| 171 | **UKG Pro WFM** | https://developer.ukg.com | `https://welcome-us.ukg.net/authorize` | — | ✓ | ⬜ |
| 172 | **Adyen** | https://docs.adyen.com | `https://ca-{environment}.adyen.com/ca/ca/oauth/connect.shtml` | — | ✓ | ⬜ |
| 173 | **Meta Marketing** | https://developers.facebook.com | _(alias: Facebook OAuth)_ | — | ✓ | ⬜ |
| 174 | **AWS Cognito** | https://console.aws.amazon.com/cognito | `https://{subdomain}.auth.{region}.amazoncognito.com/oauth2/authorize` | `openid` | ✓ | ⬜ |

---

## After Registration — Wiring In Client IDs

Once you've registered apps and saved the credentials in Bitwarden, export the vault and hand it off. The client IDs will be wired into the code:

- **High-priority services** (GitHub, Slack, Discord, etc.) → hardcoded in `oauth.rs` via env vars like `OPENPAWZ_GITHUB_CLIENT_ID`
- **Everything else** → added to `registrations.json` with env var `OPENPAWZ_{SERVICE}_CLIENT_ID`
- **No code changes needed per service** — the generic provider system handles the rest automatically

---

## Notes

- **Dynamic domains** (marked with `{subdomain}` or `{hostname}`): The user provides their instance URL at connect time. Register the app on the central developer portal — the auth/token URLs are instance-specific but the client ID is the same.
- **PKCE ✗**: These services don't support PKCE. They still work with authorization code flow but require a client secret.
- **Aliases**: YouTube = Google, Outlook = Microsoft, SharePoint = Microsoft, etc. One registration covers all.
- **Scopes**: If the Scopes column shows `—`, the default scopes are fine. Only request what's listed.

## Progress Summary

| Category | Done | Total |
|----------|------|-------|
| ✅ Registered (real client IDs) | 2 | 174 |
| Google Workspace | ✅ | — |
| Microsoft 365 | ✅ | — |
| Everything else | 0 | 172 |

## Priority Order (Register These First)

1. ~~**Google** — covers Gmail, Calendar, Drive, Sheets, YouTube~~ ✅ DONE
2. ~~**Microsoft** — covers Outlook, OneDrive, Teams, SharePoint~~ ✅ DONE
3. **GitHub** — most common developer integration
4. **Slack** — most common team chat
5. **Discord** — community platform
6. **Salesforce** — #1 CRM
7. **HubSpot** — #2 CRM
8. **Jira/Atlassian** — project management
9. **Zoom** — video conferencing
10. **Notion** — knowledge base
