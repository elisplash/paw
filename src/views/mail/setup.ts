// Mail View — Setup (account setup modal, IMAP/SMTP configuration)

import { $, escAttr } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { pawEngine } from '../../engine';
import { logCredentialActivity } from '../../db';
import { EMAIL_PROVIDERS, saveMailPermissions } from './atoms';

// ── Injected dependency (set by index.ts to break circular imports) ────────

let _loadMail: () => void = () => {};
export function setLoadMailRefSetup(fn: () => void): void {
  _loadMail = fn;
}

// ── Setup state ────────────────────────────────────────────────────────────

let _channelSetupType: string | null = null;
let closeChannelSetupFn: (() => void) | null = null;

export function getChannelSetupType(): string | null {
  return _channelSetupType;
}

export function clearChannelSetupType(): void {
  _channelSetupType = null;
}

export function setCloseChannelSetupFn(fn: () => void): void {
  closeChannelSetupFn = fn;
}

// ── Provider picker modal ──────────────────────────────────────────────────

export function openMailAccountSetup(): void {
  const title = $('channel-setup-title');
  const body = $('channel-setup-body');
  const modal = $('channel-setup-modal');
  const footer = $('channel-setup-save') as HTMLButtonElement | null;
  if (!title || !body || !modal || !footer) return;

  _channelSetupType = '__mail_imap__';
  title.textContent = 'Add Email Account';
  footer.style.display = '';
  footer.textContent = 'Connect Account';

  body.innerHTML = `
    <p class="channel-setup-desc">Choose your email provider to get started.</p>
    <div class="mail-provider-grid">
      ${Object.entries(EMAIL_PROVIDERS)
        .map(
          ([id, p]) => `
        <button class="mail-provider-btn" data-provider="${id}">
          <span class="mail-provider-icon">${p.icon}</span>
          <span class="mail-provider-name">${p.name}</span>
        </button>
      `,
        )
        .join('')}
    </div>
  `;
  footer.style.display = 'none';

  body.querySelectorAll('.mail-provider-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const providerId = btn.getAttribute('data-provider') ?? 'custom';
      showMailAccountForm(providerId);
    });
  });

  modal.style.display = '';
}

// ── Account form ───────────────────────────────────────────────────────────

function showMailAccountForm(providerId: string): void {
  const provider = EMAIL_PROVIDERS[providerId] ?? EMAIL_PROVIDERS.custom;
  const body = $('channel-setup-body');
  const footer = $('channel-setup-save') as HTMLButtonElement | null;
  const title = $('channel-setup-title');
  if (!body || !footer) return;

  if (title) title.textContent = `Connect ${provider.name}`;
  footer.style.display = '';
  footer.textContent = 'Connect Account';

  const isCustom = providerId === 'custom';
  const needsAppPw = providerId === 'gmail' || providerId === 'yahoo' || providerId === 'icloud';

  body.innerHTML = `
    <div class="mail-setup-back" id="mail-setup-back">← Choose provider</div>
    ${provider.hint ? `<div class="mail-setup-hint"><span class="ms ms-sm">info</span> ${provider.hint}</div>` : ''}
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-email">Email Address <span class="required">*</span></label>
      <input class="form-input" id="ch-field-mail-email" type="email" placeholder="you@${providerId === 'custom' ? 'example.com' : provider.imap.replace('imap.', '')}">
    </div>
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-display">Display Name</label>
      <input class="form-input" id="ch-field-mail-display" type="text" placeholder="Your Name">
      <div class="form-hint">How your name appears in outgoing emails</div>
    </div>
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-password">${needsAppPw ? 'App Password' : 'Password'} <span class="required">*</span></label>
      <input class="form-input" id="ch-field-mail-password" type="password" placeholder="${providerId === 'gmail' ? '16-character app password' : 'Password'}">
    </div>
    ${
      isCustom
        ? `
    <div class="form-row-2col">
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-imap">IMAP Server <span class="required">*</span></label>
        <input class="form-input" id="ch-field-mail-imap" type="text" placeholder="imap.example.com" value="${escAttr(provider.imap)}">
      </div>
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-imap-port">IMAP Port</label>
        <input class="form-input" id="ch-field-mail-imap-port" type="number" value="${provider.imapPort}">
      </div>
    </div>
    <div class="form-row-2col">
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-smtp">SMTP Server <span class="required">*</span></label>
        <input class="form-input" id="ch-field-mail-smtp" type="text" placeholder="smtp.example.com" value="${escAttr(provider.smtp)}">
      </div>
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-smtp-port">SMTP Port</label>
        <input class="form-input" id="ch-field-mail-smtp-port" type="number" value="${provider.smtpPort}">
      </div>
    </div>
    `
        : `
    <input type="hidden" id="ch-field-mail-imap" value="${escAttr(provider.imap)}">
    <input type="hidden" id="ch-field-mail-imap-port" value="${provider.imapPort}">
    <input type="hidden" id="ch-field-mail-smtp" value="${escAttr(provider.smtp)}">
    <input type="hidden" id="ch-field-mail-smtp-port" value="${provider.smtpPort}">
    <div class="mail-setup-servers">
      <span>IMAP: ${provider.imap}:${provider.imapPort}</span>
      <span>SMTP: ${provider.smtp}:${provider.smtpPort}</span>
    </div>
    `
    }
    <input type="hidden" id="ch-field-mail-provider" value="${providerId}">

    <div class="mail-permissions-setup">
      <div class="mail-permissions-title">Agent permissions</div>
      <div class="mail-permissions-desc">Control what your agent can do with this account. You can change these any time from the Credential Vault.</div>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-read" checked>
        <span class="mail-perm-label">Read emails</span>
        <span class="mail-perm-detail">List inbox, read messages, search</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-send" checked>
        <span class="mail-perm-label">Send emails</span>
        <span class="mail-perm-detail">Compose and send on your behalf</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-delete">
        <span class="mail-perm-label">Delete emails</span>
        <span class="mail-perm-detail">Move to trash, permanently delete</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-manage">
        <span class="mail-perm-label">Manage folders</span>
        <span class="mail-perm-detail">Create folders, move messages, flag</span>
      </label>
    </div>

    <div class="mail-security-info">
      <div class="mail-security-header">
        <span class="ms ms-sm">lock</span>
        How your credentials are stored &amp; used
      </div>
      <ul class="mail-security-list">
        <li><strong>OS keychain</strong> — your password is stored in the system keychain (macOS Keychain / libsecret on Linux), not in any file.</li>
        <li><strong>Never sent to frontend</strong> — credential details are redacted before reaching the UI.</li>
        <li><strong>TLS in transit</strong> — connections to ${provider.imap || 'your mail server'} use TLS encryption</li>
        <li><strong>No cloud</strong> — Paw and OpenClaw are fully self-hosted</li>
        <li><strong>Permission-gated</strong> — the agent must pass your Credential Vault permissions before using email tools</li>
        <li><strong>Activity log</strong> — every agent email action is recorded in a local SQLite audit log</li>
        <li><strong>Revocable</strong> — ${needsAppPw ? "revoke the app password in your provider's security settings at any time" : 'change your password to instantly revoke access'}</li>
      </ul>
    </div>
  `;

  $('mail-setup-back')?.addEventListener('click', () => openMailAccountSetup());
}

// ── Save handler ───────────────────────────────────────────────────────────

export async function saveMailImapSetup(): Promise<void> {
  console.debug('[mail-debug] saveMailImapSetup() called');
  const email = ($('ch-field-mail-email') as HTMLInputElement)?.value.trim();
  const password = ($('ch-field-mail-password') as HTMLInputElement)?.value.trim();
  console.debug('[mail-debug] email=', email, 'passwordLen=', password?.length);
  const displayName = ($('ch-field-mail-display') as HTMLInputElement)?.value.trim();
  const imapHost = ($('ch-field-mail-imap') as HTMLInputElement)?.value.trim();
  const imapPort = parseInt(($('ch-field-mail-imap-port') as HTMLInputElement)?.value ?? '993', 10);
  const smtpHost = ($('ch-field-mail-smtp') as HTMLInputElement)?.value.trim();
  const smtpPort = parseInt(($('ch-field-mail-smtp-port') as HTMLInputElement)?.value ?? '465', 10);

  if (!email) {
    showToast('Email address is required', 'error');
    return;
  }
  if (!password) {
    showToast('Password is required', 'error');
    return;
  }
  if (!imapHost) {
    showToast('IMAP server is required', 'error');
    return;
  }
  if (!smtpHost) {
    showToast('SMTP server is required', 'error');
    return;
  }

  const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = 'Connecting...';
  }

  try {
    const accountName = email.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();

    try {
      await pawEngine.mailWriteConfig({
        accountName,
        email,
        displayName: displayName || null,
        imapHost,
        imapPort,
        smtpHost,
        smtpPort,
        password,
      });
    } catch {
      // Fallback: store account info in localStorage when Tauri is not available
      console.warn(
        '[mail] Tauri runtime not available — storing account config in localStorage (no keychain)',
      );
      const existing = JSON.parse(localStorage.getItem('mail-accounts-fallback') ?? '[]') as {
        name: string;
        email: string;
        imapHost: string;
        imapPort: number;
        smtpHost: string;
        smtpPort: number;
        displayName: string;
      }[];
      const idx = existing.findIndex((a) => a.name === accountName);
      const entry = {
        name: accountName,
        email,
        imapHost,
        imapPort,
        smtpHost,
        smtpPort,
        displayName: displayName || email,
      };
      if (idx >= 0) existing[idx] = entry;
      else existing.push(entry);
      localStorage.setItem('mail-accounts-fallback', JSON.stringify(existing));
    }

    const perms = {
      read: ($('ch-field-perm-read') as HTMLInputElement)?.checked ?? true,
      send: ($('ch-field-perm-send') as HTMLInputElement)?.checked ?? true,
      delete: ($('ch-field-perm-delete') as HTMLInputElement)?.checked ?? false,
      manage: ($('ch-field-perm-manage') as HTMLInputElement)?.checked ?? false,
    };
    saveMailPermissions(accountName, perms);

    const permList = [
      perms.read && 'read',
      perms.send && 'send',
      perms.delete && 'delete',
      perms.manage && 'manage',
    ]
      .filter(Boolean)
      .join(', ');
    logCredentialActivity({
      accountName,
      action: 'approved',
      detail: `Account connected: ${email} (permissions: ${permList})`,
    });

    showToast(`${email} connected! Your agent can now read and send emails.`, 'success');
    closeChannelSetupFn?.();

    _loadMail();
  } catch (e) {
    showToast(`Failed to connect: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = 'Connect Account';
    }
  }
}
