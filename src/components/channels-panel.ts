// channels-panel.ts — Channels side panel: hero stats, connection health, kinetic init

import { $ } from './helpers';

// ── Hero stat counters ─────────────────────────────────────────────────────

export function updateChannelsHeroStats(total: number, active: number, messages: number): void {
  const elTotal = $('channels-stat-total');
  const elActive = $('channels-stat-active');
  const elMsgs = $('channels-stat-messages');
  if (elTotal) elTotal.textContent = String(total);
  if (elActive) elActive.textContent = String(active);
  if (elMsgs) elMsgs.textContent = String(messages);
}

// ── Connection Health list ─────────────────────────────────────────────────

export interface ChannelHealthEntry {
  name: string;
  icon: string;
  connected: boolean;
  messageCount?: number;
}

export function renderHealthList(entries: ChannelHealthEntry[]): void {
  const container = $('channels-health-list');
  if (!container) return;

  if (entries.length === 0) {
    container.innerHTML = '<div class="channels-health-empty">No active connections</div>';
    return;
  }

  container.innerHTML = entries
    .map(
      (e) => `
    <div class="channels-health-row">
      <div class="channels-health-dot ${e.connected ? 'online' : 'offline'}"></div>
      <span class="channels-health-name">${e.name}</span>
      <span class="channels-health-status">${e.connected ? `${e.messageCount ?? 0} msgs` : 'offline'}</span>
    </div>`,
    )
    .join('');
}

// ── Kinetic init ───────────────────────────────────────────────────────────

export function initChannelsKinetic(): void {
  const view = document.getElementById('channels-view');
  if (!view) return;

  // Stagger side panel cards
  const stagger = view.querySelector('.channels-side-panel');
  if (stagger) {
    const cards = stagger.querySelectorAll('.channels-panel-card');
    cards.forEach((card, i) => {
      (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
    });
  }
}
