// Mail View — Molecules (DOM rendering + IPC)
// Account list, email list, email detail, compose modal

import type { SkillEntry } from '../../types';
import { logCredentialActivity, getCredentialActivityLog } from '../../db';
import { $, escHtml, escAttr, formatMarkdown, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { pawEngine } from '../../engine';
import {
  type MailPermissions,
  type MailMessage,
  type MailAccount,
  loadMailPermissions,
  saveMailPermissions,
  removeMailPermissions,
  getAvatarClass,
  getInitials,
  formatMailDate,
} from './atoms';

// ── Injected dependencies (set by index.ts to break circular imports) ──────

let _openMailAccountSetup: () => void = () => {};
let _loadMail: () => void = () => {};
export function setOpenMailAccountSetup(fn: () => void): void {
  _openMailAccountSetup = fn;
}
export function setLoadMailRef(fn: () => void): void {
  _loadMail = fn;
}

// ── Module state refs (set by index.ts configure) ──────────────────────────

let _mailFolder = 'inbox';
const _mailHimalayaReady = false;
let _mailMessages: MailMessage[] = [];
let _mailSelectedId: string | null = null;
let _mailAccounts: MailAccount[] = [];
let _googleEmail: string | null = null;

let onSwitchView: ((view: string) => void) | null = null;
let onSetCurrentSession: ((key: string | null) => void) | null = null;
let getChatInput: (() => HTMLTextAreaElement | null) | null = null;

export function configureMolecules(opts: {
  switchView: (view: string) => void;
  setCurrentSession: (key: string | null) => void;
  getChatInput: () => HTMLTextAreaElement | null;
}) {
  onSwitchView = opts.switchView;
  onSetCurrentSession = opts.setCurrentSession;
  getChatInput = opts.getChatInput;
}

export function getMailAccountsRef(): MailAccount[] {
  return _mailAccounts;
}

export function setMailFolder(folder: string) {
  _mailFolder = folder;
}

export function setMailSelectedId(id: string | null) {
  _mailSelectedId = id;
}

// ── Account list rendering ─────────────────────────────────────────────────

export async function renderMailAccounts(
  _gmail: Record<string, unknown> | null,
  himalaya: SkillEntry | null,
) {
  const list = $('mail-accounts-list');
  if (!list) return;
  list.innerHTML = '';
  _mailAccounts = [];

  // ── Check Google OAuth connection ────────────────────────────────────
  try {
    const googleEmail = await pawEngine.googleOAuthStatus();
    _googleEmail = googleEmail;
    if (googleEmail) {
      _mailAccounts.push({ name: '__google__', email: googleEmail });
      const gItem = document.createElement('div');
      gItem.className = 'mail-vault-account';

      const perms = loadMailPermissions('__google__');
      const permCount = [perms.read, perms.send, perms.delete, perms.manage].filter(Boolean).length;
      const permSummary =
        [
          perms.read && 'Read',
          perms.send && 'Send',
          perms.delete && 'Delete',
          perms.manage && 'Manage',
        ]
          .filter(Boolean)
          .join(' · ') || 'No permissions';

      gItem.innerHTML = `
        <div class="mail-vault-header">
          <div class="mail-account-icon" style="background:linear-gradient(135deg,#4285F4,#34A853,#FBBC05,#EA4335);color:#fff">G</div>
          <div class="mail-account-info">
            <div class="mail-account-name">${escHtml(googleEmail)}</div>
            <div class="mail-account-status connected">${permCount}/4 permissions · Google OAuth</div>
          </div>
          <button class="btn-icon mail-vault-expand" title="Manage permissions">▾</button>
        </div>
        <div class="mail-vault-details" style="display:none">
          <div class="mail-vault-perms">
            <label class="mail-vault-perm-row">
              <input type="checkbox" class="mail-vault-cb" data-perm="read" ${perms.read ? 'checked' : ''}>
              <span class="mail-vault-perm-icon">R</span>
              <span class="mail-vault-perm-name">Read emails</span>
            </label>
            <label class="mail-vault-perm-row">
              <input type="checkbox" class="mail-vault-cb" data-perm="send" ${perms.send ? 'checked' : ''}>
              <span class="mail-vault-perm-icon">S</span>
              <span class="mail-vault-perm-name">Send emails</span>
            </label>
            <label class="mail-vault-perm-row">
              <input type="checkbox" class="mail-vault-cb" data-perm="delete" ${perms.delete ? 'checked' : ''}>
              <span class="mail-vault-perm-icon">D</span>
              <span class="mail-vault-perm-name">Delete emails</span>
            </label>
            <label class="mail-vault-perm-row">
              <input type="checkbox" class="mail-vault-cb" data-perm="manage" ${perms.manage ? 'checked' : ''}>
              <span class="mail-vault-perm-icon">F</span>
              <span class="mail-vault-perm-name">Manage folders</span>
            </label>
          </div>
          <div class="mail-vault-perm-summary">${permSummary}</div>
          <div class="mail-vault-meta">
            <span class="mail-vault-meta-item">Connected via Google OAuth2 &mdash; tokens encrypted in local vault</span>
            <span class="mail-vault-meta-item">All actions logged in Chat</span>
          </div>
          <div class="mail-vault-actions">
            <button class="btn btn-ghost btn-sm mail-google-disconnect">Disconnect Google</button>
          </div>
        </div>
      `;
      list.appendChild(gItem);

      const gExpand = gItem.querySelector('.mail-vault-expand');
      const gDetails = gItem.querySelector('.mail-vault-details') as HTMLElement;
      gExpand?.addEventListener('click', () => {
        const open = gDetails.style.display !== 'none';
        gDetails.style.display = open ? 'none' : '';
        gExpand.textContent = open ? '▾' : '▴';
      });

      gItem.querySelectorAll('.mail-vault-cb').forEach((cb) => {
        cb.addEventListener('change', () => {
          const updated: MailPermissions = {
            read: (gItem.querySelector('[data-perm="read"]') as HTMLInputElement)?.checked ?? true,
            send: (gItem.querySelector('[data-perm="send"]') as HTMLInputElement)?.checked ?? true,
            delete: (gItem.querySelector('[data-perm="delete"]') as HTMLInputElement)?.checked ?? false,
            manage: (gItem.querySelector('[data-perm="manage"]') as HTMLInputElement)?.checked ?? false,
          };
          saveMailPermissions('__google__', updated);
          const count = [updated.read, updated.send, updated.delete, updated.manage].filter(Boolean).length;
          const summary =
            [updated.read && 'Read', updated.send && 'Send', updated.delete && 'Delete', updated.manage && 'Manage']
              .filter(Boolean)
              .join(' · ') || 'No permissions';
          const statusEl = gItem.querySelector('.mail-account-status');
          const summaryEl = gItem.querySelector('.mail-vault-perm-summary');
          if (statusEl) statusEl.textContent = `${count}/4 permissions · Google OAuth`;
          if (summaryEl) summaryEl.textContent = summary;
          showToast(`Permissions updated for ${googleEmail}`, 'info');
        });
      });

      gItem.querySelector('.mail-google-disconnect')?.addEventListener('click', async () => {
        if (!(await confirmModal(`Disconnect ${googleEmail} from Google?\n\nThis removes stored OAuth tokens. Your Google account is not affected.`)))
          return;
        try {
          await pawEngine.googleOAuthDisconnect();
          removeMailPermissions('__google__');
          logCredentialActivity({
            accountName: '__google__',
            action: 'denied',
            detail: `Google account disconnected: ${googleEmail}`,
          });
          _googleEmail = null;
          showToast(`${googleEmail} disconnected`, 'success');
          _loadMail();
        } catch (err) {
          showToast(`Disconnect failed: ${err instanceof Error ? err.message : err}`, 'error');
        }
      });
    }
  } catch {
    _googleEmail = null;
    /* Google OAuth not available or not connected */
  }

  // ── Himalaya IMAP accounts ───────────────────────────────────────────

  try {
    const toml = await pawEngine.mailReadConfig();
    if (toml) {
      const accountBlocks = toml.matchAll(/\[accounts\.([^\]]+)\][\s\S]*?email\s*=\s*"([^"]+)"/g);
      for (const match of accountBlocks) {
        _mailAccounts.push({ name: match[1], email: match[2] });
      }
    }
  } catch {
    /* no config yet — try localStorage fallback */
  }

  if (_mailAccounts.length === 0) {
    try {
      const fallback = JSON.parse(
        localStorage.getItem('mail-accounts-fallback') ?? '[]',
      ) as MailAccount[];
      for (const acct of fallback) {
        _mailAccounts.push({ name: acct.name, email: acct.email });
      }
    } catch {
      /* ignore */
    }
  }

  for (const acct of _mailAccounts) {
    const perms = loadMailPermissions(acct.name);
    const item = document.createElement('div');
    item.className = 'mail-vault-account';

    const domain = acct.email.split('@')[1] ?? '';
    let icon = 'M';
    if (domain.includes('gmail')) icon = 'G';
    else if (domain.includes('outlook') || domain.includes('hotmail') || domain.includes('live'))
      icon = 'O';
    else if (domain.includes('yahoo')) icon = 'Y';
    else if (domain.includes('icloud') || domain.includes('me.com')) icon = 'iC';
    else if (domain.includes('fastmail')) icon = 'FM';

    const permCount = [perms.read, perms.send, perms.delete, perms.manage].filter(Boolean).length;
    const permSummary =
      [
        perms.read && 'Read',
        perms.send && 'Send',
        perms.delete && 'Delete',
        perms.manage && 'Manage',
      ]
        .filter(Boolean)
        .join(' · ') || 'No permissions';

    item.innerHTML = `
      <div class="mail-vault-header">
        <div class="mail-account-icon">${icon}</div>
        <div class="mail-account-info">
          <div class="mail-account-name">${escHtml(acct.email)}</div>
          <div class="mail-account-status connected">${permCount}/4 permissions active</div>
        </div>
        <button class="btn-icon mail-vault-expand" title="Manage permissions">▾</button>
      </div>
      <div class="mail-vault-details" style="display:none">
        <div class="mail-vault-perms">
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="read" ${perms.read ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">R</span>
            <span class="mail-vault-perm-name">Read emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="send" ${perms.send ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">S</span>
            <span class="mail-vault-perm-name">Send emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="delete" ${perms.delete ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">D</span>
            <span class="mail-vault-perm-name">Delete emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="manage" ${perms.manage ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">F</span>
            <span class="mail-vault-perm-name">Manage folders</span>
          </label>
        </div>
        <div class="mail-vault-perm-summary">${permSummary}</div>
        <div class="mail-vault-meta">
          <span class="mail-vault-meta-item">Stored locally at <code>~/.config/himalaya/</code> &mdash; password in OS keychain</span>
          <span class="mail-vault-meta-item">All actions logged in Chat</span>
        </div>
        <div class="mail-vault-actions">
          <button class="btn btn-ghost btn-sm mail-vault-revoke" data-account="${escAttr(acct.name)}">Revoke Access</button>
        </div>
      </div>
    `;
    list.appendChild(item);

    const expandBtn = item.querySelector('.mail-vault-expand');
    const details = item.querySelector('.mail-vault-details') as HTMLElement;
    expandBtn?.addEventListener('click', () => {
      const open = details.style.display !== 'none';
      details.style.display = open ? 'none' : '';
      expandBtn.textContent = open ? '▾' : '▴';
    });

    item.querySelectorAll('.mail-vault-cb').forEach((cb) => {
      cb.addEventListener('change', () => {
        const updated: MailPermissions = {
          read: (item.querySelector('[data-perm="read"]') as HTMLInputElement)?.checked ?? true,
          send: (item.querySelector('[data-perm="send"]') as HTMLInputElement)?.checked ?? true,
          delete:
            (item.querySelector('[data-perm="delete"]') as HTMLInputElement)?.checked ?? false,
          manage:
            (item.querySelector('[data-perm="manage"]') as HTMLInputElement)?.checked ?? false,
        };
        saveMailPermissions(acct.name, updated);
        const count = [updated.read, updated.send, updated.delete, updated.manage].filter(
          Boolean,
        ).length;
        const summary =
          [
            updated.read && 'Read',
            updated.send && 'Send',
            updated.delete && 'Delete',
            updated.manage && 'Manage',
          ]
            .filter(Boolean)
            .join(' · ') || 'No permissions';
        const statusEl = item.querySelector('.mail-account-status');
        const summaryEl = item.querySelector('.mail-vault-perm-summary');
        if (statusEl) statusEl.textContent = `${count}/4 permissions active`;
        if (summaryEl) summaryEl.textContent = summary;
        showToast(`Permissions updated for ${acct.email}`, 'info');
      });
    });

    item.querySelector('.mail-vault-revoke')?.addEventListener('click', async () => {
      if (
        !(await confirmModal(
          `Remove ${acct.email} and revoke all access?\n\nThis deletes the stored credentials from your device. Your email account is not affected.`,
        ))
      )
        return;
      try {
        await pawEngine.mailRemoveAccount(acct.name);
        removeMailPermissions(acct.name);
        logCredentialActivity({
          accountName: acct.name,
          action: 'denied',
          detail: `Account revoked: ${acct.email} — credentials deleted from device`,
        });
        showToast(`${acct.email} revoked — credentials removed from this device`, 'success');
        _loadMail();
      } catch (err) {
        showToast(`Remove failed: ${err instanceof Error ? err.message : err}`, 'error');
      }
    });
  }

  if (himalaya && (!himalaya.eligible || himalaya.disabled)) {
    const item = document.createElement('div');
    item.className = 'mail-account-item';
    const missingBins = himalaya.missing?.bins?.length;
    let statusLabel = 'Not installed';
    let statusClass = '';
    if (himalaya.disabled) {
      statusLabel = 'Disabled';
      statusClass = 'muted';
    } else if (missingBins) {
      statusLabel = 'Missing CLI';
      statusClass = 'error';
    }

    item.innerHTML = `
      <div class="mail-account-icon">H</div>
      <div class="mail-account-info">
        <div class="mail-account-name">Himalaya Skill</div>
        <div class="mail-account-status ${statusClass}">${statusLabel}</div>
      </div>
      ${himalaya.install?.length ? `<button class="btn btn-ghost btn-sm mail-himalaya-install">Install</button>` : ''}
      ${himalaya.disabled ? `<button class="btn btn-ghost btn-sm mail-himalaya-enable">Enable</button>` : ''}
    `;
    list.appendChild(item);

    item.querySelector('.mail-himalaya-install')?.addEventListener('click', async () => {
      showToast(
        'Himalaya skill installation coming soon — install manually via CLI for now',
        'info',
      );
    });
    item.querySelector('.mail-himalaya-enable')?.addEventListener('click', async () => {
      showToast('Himalaya skill management coming soon', 'info');
    });
  }

  if (_mailAccounts.length === 0 && !himalaya) {
    list.innerHTML = '<div class="mail-no-accounts">No accounts connected</div>';
  }

  renderCredentialActivityLog();
}

// ── Credential activity log ────────────────────────────────────────────────

export async function renderCredentialActivityLog() {
  let logSection = $('mail-vault-activity');
  if (!logSection) {
    const accountsSection = document.querySelector('.mail-accounts-section');
    if (!accountsSection) return;
    logSection = document.createElement('div');
    logSection.id = 'mail-vault-activity';
    logSection.className = 'mail-vault-activity-section';
    accountsSection.after(logSection);
  }

  try {
    const entries = await getCredentialActivityLog(20);
    if (entries.length === 0) {
      logSection.innerHTML = `
        <div class="mail-vault-activity-header" id="mail-vault-activity-toggle">
          <span class="ms ms-sm">description</span>
          Activity Log
          <span class="mail-vault-activity-count">0</span>
        </div>
        <div class="mail-vault-activity-empty">No credential activity yet</div>
      `;
      return;
    }

    const blocked = entries.filter((e) => !e.was_allowed).length;
    logSection.innerHTML = `
      <div class="mail-vault-activity-header" id="mail-vault-activity-toggle">
        <span class="ms ms-sm">description</span>
        Activity Log
        <span class="mail-vault-activity-count">${entries.length}${blocked ? ` · <span class="vault-blocked-count">${blocked} blocked</span>` : ''}</span>
        <span class="mail-vault-activity-chevron">▸</span>
      </div>
      <div class="mail-vault-activity-list" style="display:none">
        ${entries
          .map((e) => {
            const icon = !e.was_allowed
              ? 'X'
              : e.action === 'send'
                ? 'S'
                : e.action === 'read'
                  ? 'R'
                  : e.action === 'delete'
                    ? 'D'
                    : e.action === 'manage'
                      ? 'F'
                      : '--';
            const cls = !e.was_allowed ? 'vault-log-blocked' : '';
            const time = e.timestamp
              ? new Date(`${e.timestamp}Z`).toLocaleString([], {
                  month: 'short',
                  day: 'numeric',
                  hour: '2-digit',
                  minute: '2-digit',
                })
              : '';
            return `<div class="vault-log-entry ${cls}">
            <span class="vault-log-icon">${icon}</span>
            <div class="vault-log-body">
              <div class="vault-log-action">${escHtml(e.detail ?? e.action)}</div>
              <div class="vault-log-time">${time}${e.tool_name ? ` · ${escHtml(e.tool_name)}` : ''}</div>
            </div>
          </div>`;
          })
          .join('')}
      </div>
    `;

    $('mail-vault-activity-toggle')?.addEventListener('click', () => {
      const list = logSection!.querySelector('.mail-vault-activity-list') as HTMLElement | null;
      const chevron = logSection!.querySelector('.mail-vault-activity-chevron');
      if (list) {
        const open = list.style.display !== 'none';
        list.style.display = open ? 'none' : '';
        if (chevron) chevron.textContent = open ? '▸' : '▾';
      }
    });
  } catch {
    // DB not ready yet, skip
  }
}

// ── Inbox loading ──────────────────────────────────────────────────────────

export async function loadMailInbox() {
  _mailMessages = [];

  // ── Load Gmail messages (Google OAuth) ───────────────────────────────
  if (_googleEmail) {
    try {
      const gmailMsgs = await pawEngine.googleGmailList(undefined, 30);
      for (const gm of gmailMsgs) {
        _mailMessages.push({
          id: gm.id,
          from: gm.from || 'Unknown',
          subject: gm.subject || '(No subject)',
          snippet: gm.snippet || '',
          date: gm.date ? new Date(gm.date) : new Date(),
          read: gm.read,
          source: 'google',
        });
      }
    } catch (e) {
      console.warn('[mail] Gmail load failed:', e);
    }
  }

  // ── Load Himalaya IMAP messages ──────────────────────────────────────
  const himalayaAccount = _mailAccounts.find((a) => a.name !== '__google__');
  if (himalayaAccount) {
    try {
      const jsonResult = await pawEngine.mailFetchEmails(
        himalayaAccount.name,
        _mailFolder === 'inbox' ? 'INBOX' : _mailFolder,
        50,
      );

      interface HimalayaEnvelope {
        id: string;
        flags: string[];
        subject: string;
        from: { name?: string; addr: string };
        date: string;
      }

      let envelopes: HimalayaEnvelope[] = [];
      try {
        envelopes = JSON.parse(jsonResult);
      } catch {
        /* ignore */
      }

      for (const env of envelopes) {
        _mailMessages.push({
          id: String(env.id),
          from: env.from?.name || env.from?.addr || 'Unknown',
          subject: env.subject || '(No subject)',
          snippet: '',
          date: env.date ? new Date(env.date) : new Date(),
          read: env.flags?.includes('Seen') ?? false,
          source: 'himalaya',
        });
      }
    } catch (e) {
      console.warn('[mail] Himalaya inbox load failed:', e);
    }
  }

  // Sort all messages by date (newest first)
  _mailMessages.sort((a, b) => b.date.getTime() - a.date.getTime());

  renderMailList();
  showMailEmpty(_mailMessages.length === 0);

  const countEl = $('mail-inbox-count');
  if (countEl) countEl.textContent = String(_mailMessages.length);
}

// ── Mail list ──────────────────────────────────────────────────────────────

export function renderMailList() {
  const container = $('mail-items');
  if (!container) return;
  container.innerHTML = '';

  const filtered = _mailFolder === 'inbox' ? _mailMessages : [];

  if (_mailFolder !== 'inbox') {
    container.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);font-size:13px">
      ${_mailFolder === 'agent' ? 'Agent-drafted emails will appear here when the agent writes emails for your review.' : 'No messages in this folder.'}
    </div>`;
    return;
  }

  for (const msg of filtered) {
    const item = document.createElement('div');
    item.className = `mail-item${msg.id === _mailSelectedId ? ' active' : ''}${!msg.read ? ' unread' : ''}`;
    item.innerHTML = `
      <div class="mail-item-avatar ${getAvatarClass(msg.from)}">${getInitials(msg.from)}</div>
      <div class="mail-item-content">
        <div class="mail-item-top">
          <div class="mail-item-sender">${escHtml(msg.from)}</div>
          <div class="mail-item-date">${formatMailDate(msg.date)}</div>
        </div>
        <div class="mail-item-subject">${escHtml(msg.subject)}</div>
      </div>
    `;
    item.addEventListener('click', () => openMailMessage(msg.id));
    container.appendChild(item);
  }
}

// ── Empty state ────────────────────────────────────────────────────────────

export function showMailEmpty(show: boolean) {
  const empty = $('mail-empty');
  const items = $('mail-items');
  const chatInput = getChatInput?.();
  if (empty) {
    empty.style.display = show ? 'flex' : 'none';
    if (show) {
      const hasAccounts = _mailAccounts.length > 0;
      const hasGoogle = _googleEmail !== null;
      const mailIcon = `<div class="empty-icon"><span class="ms" style="font-size:48px">mail</span></div>`;

      if (hasAccounts && (hasGoogle || _mailHimalayaReady)) {
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Inbox is empty</div>
          <div class="empty-subtitle">No messages yet. Use Compose to send an email or ask your agent to check mail.</div>
          <button class="btn btn-ghost" id="mail-compose-cta" style="margin-top:16px">Compose Email</button>
        `;
        $('mail-compose-cta')?.addEventListener('click', () => {
          onSetCurrentSession?.(null);
          onSwitchView?.('chat');
          if (chatInput) {
            chatInput.value =
              'I want to compose a new email. Please help me draft it and send it when ready.';
            chatInput.focus();
          }
        });
      } else if (hasAccounts && !hasGoogle && !_mailHimalayaReady) {
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Enable the Himalaya skill</div>
          <div class="empty-subtitle">Your email account is configured but the Himalaya skill needs to be installed or enabled for your agent to read and send emails.</div>
          <button class="btn btn-primary" id="mail-go-skills" style="margin-top:16px">Go to Skills</button>
        `;
        $('mail-go-skills')?.addEventListener('click', () => onSwitchView?.('skills'));
      } else {
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Connect your email</div>
          <div class="empty-subtitle">Add an email account so your agent can read, draft, and send emails on your behalf.</div>
          <button class="btn btn-primary" id="mail-setup-account" style="margin-top:16px">Add Email Account</button>
        `;
        $('mail-setup-account')?.addEventListener('click', () => _openMailAccountSetup());
      }
    }
  }
  if (items) items.style.display = show ? 'none' : '';
}

// ── Email detail view ──────────────────────────────────────────────────────

export async function openMailMessage(msgId: string) {
  _mailSelectedId = msgId;
  renderMailList();

  const msg = _mailMessages.find((m) => m.id === msgId);
  const preview = $('mail-preview');
  if (!preview || !msg) return;

  // Show loading state
  preview.innerHTML = `
    <div class="mail-preview-header">
      <div class="mail-preview-avatar ${getAvatarClass(msg.from)}">${getInitials(msg.from)}</div>
      <div class="mail-preview-meta">
        <div class="mail-preview-from">${escHtml(msg.from)}</div>
        <div class="mail-preview-date">${msg.date.toLocaleString()}</div>
      </div>
    </div>
    <div class="mail-preview-subject">${escHtml(msg.subject)}</div>
    <div class="mail-preview-body" style="opacity:0.5">Loading...</div>
  `;

  // Fetch full content — route to Gmail API or Himalaya depending on source
  let body = msg.body || '';
  if (!msg.body) {
    try {
      if (msg.source === 'google') {
        const gmailMsg = await pawEngine.googleGmailRead(msgId);
        body = gmailMsg.body || gmailMsg.snippet || '';
        msg.body = body;
      } else {
        const himalayaAccount = _mailAccounts.find((a) => a.name !== '__google__');
        body = await pawEngine.mailFetchContent(himalayaAccount?.name, 'INBOX', msgId);
        msg.body = body;
      }
    } catch (e) {
      console.warn('[mail] Failed to fetch content:', e);
      body = '(Failed to load email content)';
    }
  }

  preview.innerHTML = `
    <div class="mail-preview-header">
      <div class="mail-preview-avatar ${getAvatarClass(msg.from)}">${getInitials(msg.from)}</div>
      <div class="mail-preview-meta">
        <div class="mail-preview-from">${escHtml(msg.from)}</div>
        <div class="mail-preview-date">${msg.date.toLocaleString()}</div>
      </div>
    </div>
    <div class="mail-preview-subject">${escHtml(msg.subject)}</div>
    <div class="mail-preview-body">${formatMarkdown(body)}</div>
    <div class="mail-preview-actions">
      <button class="btn btn-primary mail-action-reply">Reply</button>
      <button class="btn btn-ghost mail-action-forward">Forward</button>
      <button class="btn btn-ghost mail-action-archive">Archive</button>
      <button class="btn btn-ghost mail-action-delete">Delete</button>
    </div>
    <div class="mail-ai-actions">
      <span class="mail-ai-label">AI Actions</span>
      <button class="btn btn-sm btn-ghost mail-ai-summarize">Summarize</button>
      <button class="btn btn-sm btn-ghost mail-ai-draft">Draft Reply</button>
      <button class="btn btn-sm btn-ghost mail-ai-actions">Extract Tasks</button>
    </div>
  `;

  preview
    .querySelector('.mail-action-reply')
    ?.addEventListener('click', () => openComposeModal('reply', msg));
  preview
    .querySelector('.mail-action-forward')
    ?.addEventListener('click', () => openComposeModal('forward', msg));
  preview.querySelector('.mail-action-archive')?.addEventListener('click', () => archiveEmail(msg));
  preview.querySelector('.mail-action-delete')?.addEventListener('click', () => deleteEmail(msg));

  // AI actions
  preview
    .querySelector('.mail-ai-summarize')
    ?.addEventListener('click', () => aiMailAction('summarize', msg));
  preview
    .querySelector('.mail-ai-draft')
    ?.addEventListener('click', () => aiMailAction('draft', msg));
  preview
    .querySelector('.mail-ai-actions')
    ?.addEventListener('click', () => aiMailAction('tasks', msg));
}

// ── Compose modal ──────────────────────────────────────────────────────────

export function openComposeModal(
  mode: 'reply' | 'forward',
  msg: { from: string; subject: string; body?: string; source?: 'himalaya' | 'google' },
) {
  const modal = document.createElement('div');
  modal.className = 'mail-compose-modal';
  modal.innerHTML = `
    <div class="mail-compose-dialog">
      <div class="mail-compose-header">
        <span>${mode === 'reply' ? 'Reply' : 'Forward'}</span>
        <button class="btn-icon mail-compose-close">×</button>
      </div>
      <div class="mail-compose-body">
        <input type="text" class="mail-compose-to" placeholder="To" value="${mode === 'reply' ? escAttr(msg.from) : ''}">
        <input type="text" class="mail-compose-subject" placeholder="Subject" value="${mode === 'reply' ? 'Re: ' : 'Fwd: '}${escAttr(msg.subject)}">
        <textarea class="mail-compose-content" placeholder="Write your message...">${mode === 'forward' ? `\n\n--- Forwarded ---\n${msg.body || ''}` : ''}</textarea>
      </div>
      <div class="mail-compose-footer">
        <button class="btn btn-ghost mail-compose-cancel">Cancel</button>
        <button class="btn btn-primary mail-compose-send">Send</button>
      </div>
    </div>
  `;
  document.body.appendChild(modal);

  const close = () => modal.remove();
  modal.querySelector('.mail-compose-close')?.addEventListener('click', close);
  modal.querySelector('.mail-compose-cancel')?.addEventListener('click', close);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) close();
  });

  modal.querySelector('.mail-compose-send')?.addEventListener('click', async () => {
    const to = (modal.querySelector('.mail-compose-to') as HTMLInputElement)?.value;
    const subject = (modal.querySelector('.mail-compose-subject') as HTMLInputElement)?.value;
    const body = (modal.querySelector('.mail-compose-content') as HTMLTextAreaElement)?.value;
    if (!to || !subject) {
      showToast('Please fill in To and Subject', 'error');
      return;
    }

    try {
      // Use Gmail API if this is a Google message, otherwise Himalaya
      if (msg.source === 'google' || (!msg.source && _googleEmail)) {
        await pawEngine.googleGmailSend(to, subject, body);
      } else {
        const himalayaAccount = _mailAccounts.find((a) => a.name !== '__google__');
        await pawEngine.mailSend(himalayaAccount?.name, to, subject, body);
      }
      showToast('Email sent!', 'success');
      close();
    } catch (e) {
      showToast(`Failed to send: ${e}`, 'error');
    }
  });
}

// ── Email actions ──────────────────────────────────────────────────────────

async function archiveEmail(msg: { id: string; source?: 'himalaya' | 'google' }) {
  try {
    if (msg.source === 'google') {
      // Archive via Gmail API (remove INBOX label)
      showToast('Gmail archive — coming soon', 'info');
    } else {
      const himalayaAccount = _mailAccounts.find((a) => a.name !== '__google__');
      await pawEngine.mailMove(himalayaAccount?.name, msg.id, '[Gmail]/All Mail');
    }
    showToast('Archived', 'success');
    _mailMessages = _mailMessages.filter((m) => m.id !== msg.id);
    renderMailList();
    const preview = $('mail-preview');
    if (preview)
      preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
  } catch (e) {
    showToast(`Archive failed: ${e}`, 'error');
  }
}

async function deleteEmail(msg: { id: string; subject: string; source?: 'himalaya' | 'google' }) {
  if (!(await confirmModal(`Delete "${msg.subject}"?`))) return;
  try {
    if (msg.source === 'google') {
      showToast('Gmail delete — coming soon', 'info');
    } else {
      const himalayaAccount = _mailAccounts.find((a) => a.name !== '__google__');
      await pawEngine.mailDelete(himalayaAccount?.name, msg.id);
    }
    showToast('Deleted', 'success');
    _mailMessages = _mailMessages.filter((m) => m.id !== msg.id);
    renderMailList();
    const preview = $('mail-preview');
    if (preview)
      preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
  } catch (e) {
    showToast(`Delete failed: ${e}`, 'error');
  }
}

function aiMailAction(
  action: 'summarize' | 'draft' | 'tasks',
  msg: { from: string; subject: string; body?: string },
) {
  const prompts: Record<string, string> = {
    summarize: `Summarize this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`,
    draft: `Draft a professional reply to this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`,
    tasks: `Extract any action items or tasks from this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`,
  };

  onSetCurrentSession?.(null);
  onSwitchView?.('chat');
  const chatInput = getChatInput?.();
  if (chatInput) {
    chatInput.value = prompts[action];
    chatInput.focus();
    chatInput.form?.requestSubmit();
  }
}
