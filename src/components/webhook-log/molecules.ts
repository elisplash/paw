// molecules.ts — Webhook event log rendering and Tauri event listener

import { escHtml } from '../helpers';
import { relativeTime } from '../../views/today/atoms';
import { parseWebhookEvent, truncatePreview, type WebhookLogEntry } from './atoms';

const $ = (id: string) => document.getElementById(id);

let _entries: WebhookLogEntry[] = [];
const MAX_ENTRIES = 100;

/** Add an entry and re-render. */
export function addWebhookEntry(payload: Record<string, unknown>) {
  const entry = parseWebhookEvent(payload);
  _entries.unshift(entry);
  if (_entries.length > MAX_ENTRIES) _entries = _entries.slice(0, MAX_ENTRIES);
  renderLog();
}

/** Clear the log. */
export function clearWebhookLog() {
  _entries = [];
  renderLog();
}

function renderLog() {
  const container = $('webhook-log-list');
  if (!container) return;

  if (_entries.length === 0) {
    container.innerHTML = `<div class="webhook-log-empty">No webhook events received yet</div>`;
    return;
  }

  container.innerHTML = _entries
    .map((e) => {
      const time = relativeTime(e.timestamp);
      const preview = truncatePreview(e.messagePreview, 100);
      return `<div class="webhook-log-item">
        <div class="webhook-log-meta">
          <span class="webhook-log-agent">${escHtml(e.agentId)}</span>
          <span class="webhook-log-peer">${escHtml(e.peer)}</span>
          <span class="webhook-log-time">${time}</span>
        </div>
        <div class="webhook-log-preview">${escHtml(preview)}</div>
      </div>`;
    })
    .join('');
}

/** Initialise the webhook log listener. Uses Tauri listen API if available. */
export function initWebhookLog() {
  const tauriWindow = window as unknown as {
    __TAURI__?: {
      event: {
        listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
      };
    };
  };

  const listen = tauriWindow.__TAURI__?.event?.listen;
  if (listen) {
    listen<Record<string, unknown>>('webhook-activity', (event) => {
      addWebhookEntry(event.payload);
    }).catch((e) => console.warn('[webhook-log] listen error:', e));
  }
}
