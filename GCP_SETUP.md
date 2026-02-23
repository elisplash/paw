# Google Cloud Setup — Remaining Tasks

## 1. Publish the OAuth Consent Screen

Your GCP project "OpenPawz" is currently in **Testing** mode, which means:
- Only manually-added test users can connect (max 100)
- Refresh tokens expire after **7 days** — users get disconnected weekly

**Steps:**
1. Go to [OAuth consent screen](https://console.cloud.google.com/apis/credentials/consent?project=openpawz)
2. Click **"Publish App"**
3. Click **"Confirm"** on the warning
4. Users will see a "This app isn't verified" warning but can click **Advanced → Go to OpenPawz** to proceed

**Optional (later):** Submit for Google verification review to remove the warning. Requires a homepage URL, privacy policy, and a YouTube demo video. Takes 2–6 weeks.

---

## 2. Add GitHub Secrets for Release Builds

The bundled OAuth credentials are baked into the binary at compile time. Your local `src-tauri/.env` works for dev builds, but CI/release builds need GitHub Actions secrets.

**Steps:**
1. Go to [Repo Settings → Secrets → Actions](https://github.com/elisplash/paw/settings/secrets/actions)
2. Add these two repository secrets:

| Secret Name | Value |
|-------------|-------|
| `PAW_GOOGLE_CLIENT_ID` | *(from your GCP Console → Credentials → OAuth 2.0 Client ID)* |
| `PAW_GOOGLE_CLIENT_SECRET` | *(from your GCP Console → Credentials → OAuth 2.0 Client Secret)* |

3. In your future release workflow, pass them as env vars:
```yaml
env:
  PAW_GOOGLE_CLIENT_ID: ${{ secrets.PAW_GOOGLE_CLIENT_ID }}
  PAW_GOOGLE_CLIENT_SECRET: ${{ secrets.PAW_GOOGLE_CLIENT_SECRET }}
```
