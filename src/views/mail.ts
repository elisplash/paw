// Mail View — Email via Gmail Hooks + Himalaya
// Extracted from main.ts for maintainability

import type { SkillEntry } from '../types';
import { logCredentialActivity, getCredentialActivityLog } from '../db';
import { $, escHtml, escAttr, formatMarkdown } from '../components/helpers';
import { showToast } from '../components/toast';
import { isConnected } from '../state/connection';

// ── Tauri bridge ───────────────────────────────────────────────────────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const invoke = tauriWindow.__TAURI__?.core?.invoke;

// ── Module state ───────────────────────────────────────────────────────────
let _mailFolder = 'inbox';
let _mailGmailConfigured = false;
let _mailHimalayaReady = false;
let _mailMessages: { id: string; from: string; subject: string; snippet: string; date: Date; body?: string; sessionKey?: string; read?: boolean }[] = [];
let _mailSelectedId: string | null = null;
let _mailAccounts: { name: string; email: string }[] = [];
let _channelSetupType: string | null = null;

export function getMailAccounts(): { name: string; email: string }[] {
  return _mailAccounts;
}

// Callbacks for main.ts integration
let onSwitchView: ((view: string) => void) | null = null;
let onSetCurrentSession: ((key: string | null) => void) | null = null;
let getChatInput: (() => HTMLTextAreaElement | null) | null = null;

export function configure(opts: {
  switchView: (view: string) => void;
  setCurrentSession: (key: string | null) => void;
  getChatInput: () => HTMLTextAreaElement | null;
  closeChannelSetup: () => void;
}) {
  onSwitchView = opts.switchView;
  onSetCurrentSession = opts.setCurrentSession;
  getChatInput = opts.getChatInput;
  closeChannelSetupFn = opts.closeChannelSetup;
}

let closeChannelSetupFn: (() => void) | null = null;


function _extractContent(content: unknown): string {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content.map(c => (c as { text?: string }).text ?? '').join('');
  }
  return '';
}
void _extractContent;

// ── Mail permissions ───────────────────────────────────────────────────────
export interface MailPermissions {
  read: boolean;
  send: boolean;
  delete: boolean;
  manage: boolean;
}

export function loadMailPermissions(accountName: string): MailPermissions {
  try {
    const raw = localStorage.getItem(`mail-perms-${accountName}`);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return { read: true, send: true, delete: false, manage: false };
}

function saveMailPermissions(accountName: string, perms: MailPermissions) {
  localStorage.setItem(`mail-perms-${accountName}`, JSON.stringify(perms));
}

function removeMailPermissions(accountName: string) {
  localStorage.removeItem(`mail-perms-${accountName}`);
}

// ── Main loader ────────────────────────────────────────────────────────────
export async function loadMail() {
  if (!isConnected()) {
    console.warn('[mail] loadMail skipped — engine not connected');
    // Still try to render local accounts so user sees their config
    await renderMailAccounts(null, null);
    if (_mailAccounts.length === 0) {
      showMailEmpty(true);
    }
    return;
  }
  try {
    // Skills status check — engine doesn't have a skills API yet
    // Just render local accounts from Himalaya config
    await renderMailAccounts(null, null);

    const hasAccounts = _mailAccounts.length > 0 || _mailGmailConfigured;
    if (hasAccounts) {
      await loadMailInbox();
    } else {
      _mailMessages = [];
      renderMailList();
      showMailEmpty(true);
    }
  } catch (e) {
    console.warn('[mail] Load failed:', e);
    showMailEmpty(true);
  }
}

async function renderMailAccounts(_gmail: Record<string, unknown> | null, himalaya: SkillEntry | null) {
  const list = $('mail-accounts-list');
  if (!list) return;
  list.innerHTML = '';
  _mailAccounts = [];

  if (invoke) {
    try {
      const toml = await invoke<string>('read_himalaya_config');
      if (toml) {
        const accountBlocks = toml.matchAll(/\[accounts\.([^\]]+)\][\s\S]*?email\s*=\s*"([^"]+)"/g);
        for (const match of accountBlocks) {
          _mailAccounts.push({ name: match[1], email: match[2] });
        }
      }
    } catch { /* no config yet */ }
  } else {
    // Fallback: read from localStorage when Tauri is unavailable
    try {
      const fallback = JSON.parse(localStorage.getItem('mail-accounts-fallback') ?? '[]') as { name: string; email: string }[];
      for (const acct of fallback) {
        _mailAccounts.push({ name: acct.name, email: acct.email });
      }
    } catch { /* ignore */ }
  }

  for (const acct of _mailAccounts) {
    const perms = loadMailPermissions(acct.name);
    const item = document.createElement('div');
    item.className = 'mail-vault-account';

    const domain = acct.email.split('@')[1] ?? '';
    let icon = 'M';
    if (domain.includes('gmail')) icon = 'G';
    else if (domain.includes('outlook') || domain.includes('hotmail') || domain.includes('live')) icon = 'O';
    else if (domain.includes('yahoo')) icon = 'Y';
    else if (domain.includes('icloud') || domain.includes('me.com')) icon = 'iC';
    else if (domain.includes('fastmail')) icon = 'FM';

    const permCount = [perms.read, perms.send, perms.delete, perms.manage].filter(Boolean).length;
    const permSummary = [perms.read && 'Read', perms.send && 'Send', perms.delete && 'Delete', perms.manage && 'Manage'].filter(Boolean).join(' · ') || 'No permissions';

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

    item.querySelectorAll('.mail-vault-cb').forEach(cb => {
      cb.addEventListener('change', () => {
        const updated: MailPermissions = {
          read: (item.querySelector('[data-perm="read"]') as HTMLInputElement)?.checked ?? true,
          send: (item.querySelector('[data-perm="send"]') as HTMLInputElement)?.checked ?? true,
          delete: (item.querySelector('[data-perm="delete"]') as HTMLInputElement)?.checked ?? false,
          manage: (item.querySelector('[data-perm="manage"]') as HTMLInputElement)?.checked ?? false,
        };
        saveMailPermissions(acct.name, updated);
        const count = [updated.read, updated.send, updated.delete, updated.manage].filter(Boolean).length;
        const summary = [updated.read && 'Read', updated.send && 'Send', updated.delete && 'Delete', updated.manage && 'Manage'].filter(Boolean).join(' · ') || 'No permissions';
        const statusEl = item.querySelector('.mail-account-status');
        const summaryEl = item.querySelector('.mail-vault-perm-summary');
        if (statusEl) statusEl.textContent = `${count}/4 permissions active`;
        if (summaryEl) summaryEl.textContent = summary;
        showToast(`Permissions updated for ${acct.email}`, 'info');
      });
    });

    item.querySelector('.mail-vault-revoke')?.addEventListener('click', async () => {
      if (!confirm(`Remove ${acct.email} and revoke all access?\n\nThis deletes the stored credentials from your device. Your email account is not affected.`)) return;
      try {
        if (invoke) await invoke('remove_himalaya_account', { accountName: acct.name });
        removeMailPermissions(acct.name);
        logCredentialActivity({
          accountName: acct.name,
          action: 'denied',
          detail: `Account revoked: ${acct.email} — credentials deleted from device`,
        });
        showToast(`${acct.email} revoked — credentials removed from this device`, 'success');
        loadMail();
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
    if (himalaya.disabled) { statusLabel = 'Disabled'; statusClass = 'muted'; }
    else if (missingBins) { statusLabel = 'Missing CLI'; statusClass = 'error'; }

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
      showToast('Himalaya skill installation coming soon — install manually via CLI for now', 'info');
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

async function renderCredentialActivityLog() {
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

    const blocked = entries.filter(e => !e.was_allowed).length;
    logSection.innerHTML = `
      <div class="mail-vault-activity-header" id="mail-vault-activity-toggle">
        <span class="ms ms-sm">description</span>
        Activity Log
        <span class="mail-vault-activity-count">${entries.length}${blocked ? ` · <span class="vault-blocked-count">${blocked} blocked</span>` : ''}</span>
        <span class="mail-vault-activity-chevron">▸</span>
      </div>
      <div class="mail-vault-activity-list" style="display:none">
        ${entries.map(e => {
          const icon = !e.was_allowed ? 'X' : e.action === 'send' ? 'S' : e.action === 'read' ? 'R' : e.action === 'delete' ? 'D' : e.action === 'manage' ? 'F' : '--';
          const cls = !e.was_allowed ? 'vault-log-blocked' : '';
          const time = e.timestamp ? new Date(e.timestamp + 'Z').toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '';
          return `<div class="vault-log-entry ${cls}">
            <span class="vault-log-icon">${icon}</span>
            <div class="vault-log-body">
              <div class="vault-log-action">${escHtml(e.detail ?? e.action)}</div>
              <div class="vault-log-time">${time}${e.tool_name ? ' · ' + escHtml(e.tool_name) : ''}</div>
            </div>
          </div>`;
        }).join('')}
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

async function loadMailInbox() {
  if (!invoke) {
    console.warn('[mail] Tauri invoke not available');
    _mailMessages = [];
    renderMailList();
    showMailEmpty(true);
    return;
  }

  const accountName = _mailAccounts.length > 0 ? _mailAccounts[0].name : undefined;

  try {
    const jsonResult = await invoke<string>('fetch_emails', {
      account: accountName,
      folder: _mailFolder === 'inbox' ? 'INBOX' : _mailFolder,
      pageSize: 50
    });

    interface HimalayaEnvelope {
      id: string;
      flags: string[];
      subject: string;
      from: { name?: string; addr: string };
      date: string;
    }

    let envelopes: HimalayaEnvelope[] = [];
    try { envelopes = JSON.parse(jsonResult); } catch { /* ignore */ }

    _mailMessages = envelopes.map(env => ({
      id: String(env.id),
      from: env.from?.name || env.from?.addr || 'Unknown',
      subject: env.subject || '(No subject)',
      snippet: '',
      date: env.date ? new Date(env.date) : new Date(),
      read: env.flags?.includes('Seen') ?? false,
    })).sort((a, b) => b.date.getTime() - a.date.getTime());

    renderMailList();
    showMailEmpty(_mailMessages.length === 0);

    const countEl = $('mail-inbox-count');
    if (countEl) countEl.textContent = String(_mailMessages.length);
  } catch (e) {
    console.warn('[mail] Inbox load failed:', e);
    _mailMessages = [];
    renderMailList();
    showMailEmpty(true);
  }
}

function showMailEmpty(show: boolean) {
  const empty = $('mail-empty');
  const items = $('mail-items');
  const chatInput = getChatInput?.();
  if (empty) {
    empty.style.display = show ? 'flex' : 'none';
    if (show) {
      const hasAccounts = _mailAccounts.length > 0;
      const mailIcon = `<div class="empty-icon"><span class="ms" style="font-size:48px">mail</span></div>`;

      if (hasAccounts && _mailHimalayaReady) {
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Inbox is empty</div>
          <div class="empty-subtitle">No messages yet. Use Compose to send an email or ask your agent to check mail.</div>
          <button class="btn btn-ghost" id="mail-compose-cta" style="margin-top:16px">Compose Email</button>
        `;
        $('mail-compose-cta')?.addEventListener('click', () => {
          onSetCurrentSession?.(null);
          onSwitchView?.('chat');
          if (chatInput) { chatInput.value = 'I want to compose a new email. Please help me draft it and use himalaya to send it when ready.'; chatInput.focus(); }
        });
      } else if (hasAccounts && !_mailHimalayaReady) {
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
        $('mail-setup-account')?.addEventListener('click', () => openMailAccountSetup());
      }
    }
  }
  if (items) items.style.display = show ? 'none' : '';
}

function getAvatarClass(sender: string): string {
  const colors = ['', 'avatar-green', 'avatar-purple', 'avatar-orange', 'avatar-pink'];
  if (sender.toLowerCase().includes('google')) return 'avatar-google';
  if (sender.toLowerCase().includes('microsoft')) return 'avatar-microsoft';
  const hash = sender.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  return colors[hash % colors.length];
}

function getInitials(sender: string): string {
  const name = sender.replace(/<.*>/, '').trim();
  if (!name) return '?';
  const parts = name.split(/[\s@._-]+/).filter(p => p.length > 0);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase();
}

function renderMailList() {
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

function formatMailDate(date: Date): string {
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  if (diff < 86400000 && now.getDate() === date.getDate()) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  if (diff < 604800000) {
    return date.toLocaleDateString([], { weekday: 'short' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

async function openMailMessage(msgId: string) {
  _mailSelectedId = msgId;
  renderMailList();

  const msg = _mailMessages.find(m => m.id === msgId);
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

  // Fetch full content via himalaya
  let body = msg.body || '';
  if (invoke && !msg.body) {
    try {
      const accountName = _mailAccounts.length > 0 ? _mailAccounts[0].name : undefined;
      body = await invoke<string>('fetch_email_content', {
        account: accountName,
        folder: 'INBOX',
        id: msgId
      });
      msg.body = body;
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

  // Reply - open compose in mail view
  preview.querySelector('.mail-action-reply')?.addEventListener('click', () => openComposeModal('reply', msg));
  preview.querySelector('.mail-action-forward')?.addEventListener('click', () => openComposeModal('forward', msg));
  preview.querySelector('.mail-action-archive')?.addEventListener('click', () => archiveEmail(msg));
  preview.querySelector('.mail-action-delete')?.addEventListener('click', () => deleteEmail(msg));
  
  // AI actions
  preview.querySelector('.mail-ai-summarize')?.addEventListener('click', () => aiMailAction('summarize', msg));
  preview.querySelector('.mail-ai-draft')?.addEventListener('click', () => aiMailAction('draft', msg));
  preview.querySelector('.mail-ai-actions')?.addEventListener('click', () => aiMailAction('tasks', msg));
}

// Compose modal for reply/forward
function openComposeModal(mode: 'reply' | 'forward', msg: { from: string; subject: string; body?: string }) {
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
        <textarea class="mail-compose-content" placeholder="Write your message...">${mode === 'forward' ? '\n\n--- Forwarded ---\n' + (msg.body || '') : ''}</textarea>
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
  modal.addEventListener('click', (e) => { if (e.target === modal) close(); });

  modal.querySelector('.mail-compose-send')?.addEventListener('click', async () => {
    const to = (modal.querySelector('.mail-compose-to') as HTMLInputElement)?.value;
    const subject = (modal.querySelector('.mail-compose-subject') as HTMLInputElement)?.value;
    const body = (modal.querySelector('.mail-compose-content') as HTMLTextAreaElement)?.value;
    if (!to || !subject) { showToast('Please fill in To and Subject', 'error'); return; }
    
    try {
      const accountName = _mailAccounts.length > 0 ? _mailAccounts[0].name : undefined;
      await invoke?.('send_email', { account: accountName, to, subject, body });
      showToast('Email sent!', 'success');
      close();
    } catch (e) {
      showToast(`Failed to send: ${e}`, 'error');
    }
  });
}

async function archiveEmail(msg: { id: string }) {
  try {
    // Move to archive folder via himalaya
    const accountName = _mailAccounts.length > 0 ? _mailAccounts[0].name : undefined;
    // himalaya message move <id> <folder>
    await invoke?.('move_email', { account: accountName, id: msg.id, folder: '[Gmail]/All Mail' });
    showToast('Archived', 'success');
    _mailMessages = _mailMessages.filter(m => m.id !== msg.id);
    renderMailList();
    const preview = $('mail-preview');
    if (preview) preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
  } catch (e) {
    showToast(`Archive failed: ${e}`, 'error');
  }
}

async function deleteEmail(msg: { id: string; subject: string }) {
  if (!confirm(`Delete "${msg.subject}"?`)) return;
  try {
    const accountName = _mailAccounts.length > 0 ? _mailAccounts[0].name : undefined;
    await invoke?.('delete_email', { account: accountName, id: msg.id });
    showToast('Deleted', 'success');
    _mailMessages = _mailMessages.filter(m => m.id !== msg.id);
    renderMailList();
    const preview = $('mail-preview');
    if (preview) preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
  } catch (e) {
    showToast(`Delete failed: ${e}`, 'error');
  }
}

async function aiMailAction(action: 'summarize' | 'draft' | 'tasks', msg: { from: string; subject: string; body?: string }) {
  const prompts: Record<string, string> = {
    summarize: `Summarize this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`,
    draft: `Draft a professional reply to this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`,
    tasks: `Extract any action items or tasks from this email from ${msg.from}:\n\nSubject: ${msg.subject}\n\n${msg.body || ''}`
  };
  
  // Send to chat and switch view
  onSetCurrentSession?.(null);
  onSwitchView?.('chat');
  const chatInput = getChatInput?.();
  if (chatInput) {
    chatInput.value = prompts[action];
    chatInput.focus();
    // Auto-submit
    chatInput.form?.requestSubmit();
  }
}

function _composeMailReply(msg: { from: string; subject: string; sessionKey?: string }) {
  openComposeModal('reply', msg);
}
void _composeMailReply;

// ── Provider presets ───────────────────────────────────────────────────────
const EMAIL_PROVIDERS: Record<string, { name: string; icon: string; imap: string; imapPort: number; smtp: string; smtpPort: number; hint: string }> = {
  gmail: { name: 'Gmail', icon: 'G', imap: 'imap.gmail.com', imapPort: 993, smtp: 'smtp.gmail.com', smtpPort: 465, hint: 'Use an App Password — go to Google Account → Security → App Passwords' },
  outlook: { name: 'Outlook / Hotmail', icon: 'O', imap: 'outlook.office365.com', imapPort: 993, smtp: 'smtp.office365.com', smtpPort: 587, hint: 'Use your regular password, or an App Password if 2FA is on' },
  yahoo: { name: 'Yahoo Mail', icon: 'Y', imap: 'imap.mail.yahoo.com', imapPort: 993, smtp: 'smtp.mail.yahoo.com', smtpPort: 465, hint: 'Generate an App Password in Yahoo Account Settings → Security' },
  icloud: { name: 'iCloud Mail', icon: 'iC', imap: 'imap.mail.me.com', imapPort: 993, smtp: 'smtp.mail.me.com', smtpPort: 587, hint: 'Use an App-Specific Password from appleid.apple.com' },
  fastmail: { name: 'Fastmail', icon: 'FM', imap: 'imap.fastmail.com', imapPort: 993, smtp: 'smtp.fastmail.com', smtpPort: 465, hint: 'Use an App Password from Settings → Privacy & Security' },
  custom: { name: 'Other (IMAP/SMTP)', icon: '*', imap: '', imapPort: 993, smtp: '', smtpPort: 465, hint: 'Enter your mail server details manually' },
};

export function openMailAccountSetup() {
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
      ${Object.entries(EMAIL_PROVIDERS).map(([id, p]) => `
        <button class="mail-provider-btn" data-provider="${id}">
          <span class="mail-provider-icon">${p.icon}</span>
          <span class="mail-provider-name">${p.name}</span>
        </button>
      `).join('')}
    </div>
  `;
  footer.style.display = 'none';

  body.querySelectorAll('.mail-provider-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const providerId = btn.getAttribute('data-provider') ?? 'custom';
      showMailAccountForm(providerId);
    });
  });

  modal.style.display = '';
}

function showMailAccountForm(providerId: string) {
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
    ${isCustom ? `
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
    ` : `
    <input type="hidden" id="ch-field-mail-imap" value="${escAttr(provider.imap)}">
    <input type="hidden" id="ch-field-mail-imap-port" value="${provider.imapPort}">
    <input type="hidden" id="ch-field-mail-smtp" value="${escAttr(provider.smtp)}">
    <input type="hidden" id="ch-field-mail-smtp-port" value="${provider.smtpPort}">
    <div class="mail-setup-servers">
      <span>IMAP: ${provider.imap}:${provider.imapPort}</span>
      <span>SMTP: ${provider.smtp}:${provider.smtpPort}</span>
    </div>
    `}
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

export async function saveMailImapSetup() {
  console.log('[mail-debug] saveMailImapSetup() called');
  const email = ($('ch-field-mail-email') as HTMLInputElement)?.value.trim();
  const password = ($('ch-field-mail-password') as HTMLInputElement)?.value.trim();
  console.log('[mail-debug] email=', email, 'passwordLen=', password?.length, 'invoke available=', !!invoke);
  const displayName = ($('ch-field-mail-display') as HTMLInputElement)?.value.trim();
  const imapHost = ($('ch-field-mail-imap') as HTMLInputElement)?.value.trim();
  const imapPort = parseInt(($('ch-field-mail-imap-port') as HTMLInputElement)?.value ?? '993', 10);
  const smtpHost = ($('ch-field-mail-smtp') as HTMLInputElement)?.value.trim();
  const smtpPort = parseInt(($('ch-field-mail-smtp-port') as HTMLInputElement)?.value ?? '465', 10);

  if (!email) { showToast('Email address is required', 'error'); return; }
  if (!password) { showToast('Password is required', 'error'); return; }
  if (!imapHost) { showToast('IMAP server is required', 'error'); return; }
  if (!smtpHost) { showToast('SMTP server is required', 'error'); return; }

  const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
  if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Connecting...'; }

  try {
    const accountName = email.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();

    if (invoke) {
      await invoke('write_himalaya_config', {
        accountName,
        email,
        displayName: displayName || null,
        imapHost,
        imapPort,
        smtpHost,
        smtpPort,
        password,
      });
    } else {
      // Fallback: store account info in localStorage when Tauri is not available
      // (e.g. running in dev browser mode). The password is NOT stored in this case.
      console.warn('[mail] Tauri runtime not available — storing account config in localStorage (no keychain)');
      const existing = JSON.parse(localStorage.getItem('mail-accounts-fallback') ?? '[]') as { name: string; email: string; imapHost: string; imapPort: number; smtpHost: string; smtpPort: number; displayName: string }[];
      const idx = existing.findIndex(a => a.name === accountName);
      const entry = { name: accountName, email, imapHost, imapPort, smtpHost, smtpPort, displayName: displayName || email };
      if (idx >= 0) existing[idx] = entry; else existing.push(entry);
      localStorage.setItem('mail-accounts-fallback', JSON.stringify(existing));
    }

    const perms = {
      read: ($('ch-field-perm-read') as HTMLInputElement)?.checked ?? true,
      send: ($('ch-field-perm-send') as HTMLInputElement)?.checked ?? true,
      delete: ($('ch-field-perm-delete') as HTMLInputElement)?.checked ?? false,
      manage: ($('ch-field-perm-manage') as HTMLInputElement)?.checked ?? false,
    };
    saveMailPermissions(accountName, perms);

    const permList = [perms.read && 'read', perms.send && 'send', perms.delete && 'delete', perms.manage && 'manage'].filter(Boolean).join(', ');
    logCredentialActivity({
      accountName,
      action: 'approved',
      detail: `Account connected: ${email} (permissions: ${permList})`,
    });

    showToast(`${email} connected! Your agent can now read and send emails.`, 'success');
    closeChannelSetupFn?.();

    setTimeout(() => loadMail(), 500);
  } catch (e) {
    showToast(`Failed to connect: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Connect Account'; }
  }
}

export function getChannelSetupType(): string | null {
  return _channelSetupType;
}

export function clearChannelSetupType() {
  _channelSetupType = null;
}

// ── Event wiring ───────────────────────────────────────────────────────────
export function initMailEvents() {
  // Compose new email
  $('mail-compose')?.addEventListener('click', () => {
    const chatInput = getChatInput?.();
    if (!_mailHimalayaReady) {
      showToast('Himalaya skill is required to send emails. Enable it in the Skills view.', 'error');
      return;
    }
    const prompt = 'I want to compose a new email. Please help me draft it and use himalaya to send it when ready.';
    onSetCurrentSession?.(null);
    onSwitchView?.('chat');
    if (chatInput) {
      chatInput.value = prompt;
      chatInput.focus();
    }
  });

  // Mail folder switching
  document.querySelectorAll('.mail-folder').forEach(folder => {
    folder.addEventListener('click', () => {
      document.querySelectorAll('.mail-folder').forEach(f => f.classList.remove('active'));
      folder.classList.add('active');
      _mailFolder = folder.getAttribute('data-folder') ?? 'inbox';
      const titleEl = $('mail-folder-title');
      if (titleEl) {
        const labels: Record<string, string> = { inbox: 'Inbox', drafts: 'Drafts', sent: 'Sent', agent: 'Agent Drafts' };
        titleEl.textContent = labels[_mailFolder] ?? _mailFolder;
      }
      renderMailList();
      const preview = $('mail-preview');
      if (preview) preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
      _mailSelectedId = null;
    });
  });

  // Refresh
  $('mail-refresh')?.addEventListener('click', () => loadMail());

  // Add account buttons
  $('mail-add-account')?.addEventListener('click', () => openMailAccountSetup());
}