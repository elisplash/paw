// molecules.ts — Notifications drawer rendering, state management, persistence

import { escHtml } from '../helpers';
import { relativeTime } from '../../views/today/atoms';
import { badgePulse } from '../animations';
import {
  notificationIcon,
  countUnread,
  markRead,
  markAllRead,
  type Notification,
  type NotificationKind,
  createNotificationId,
} from './atoms';

const $ = (id: string) => document.getElementById(id);

const STORAGE_KEY = 'pawz-notifications';
const MAX_NOTIFICATIONS = 50;

let _notifications: Notification[] = [];
let _isOpen = false;

// ── Persistence ──────────────────────────────────────────────────────────

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(_notifications));
  } catch {
    // Storage full or unavailable — silently ignore
  }
}

function loadFromStorage(): Notification[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as Notification[];
    if (!Array.isArray(parsed)) return [];
    return parsed.slice(0, MAX_NOTIFICATIONS);
  } catch {
    return [];
  }
}

// ── Push notification ────────────────────────────────────────────────────

/** Add a new notification and pulse the badge. */
export function pushNotification(
  kind: NotificationKind,
  title: string,
  body?: string,
  agent?: string,
  navigateTo?: string,
) {
  _notifications.unshift({
    id: createNotificationId(),
    kind,
    title,
    body,
    agent,
    timestamp: new Date().toISOString(),
    read: false,
    navigateTo,
  });
  // Cap at max
  if (_notifications.length > MAX_NOTIFICATIONS)
    _notifications = _notifications.slice(0, MAX_NOTIFICATIONS);
  updateBadge();
  persist();
  if (_isOpen) renderList();

  // Pulse the badge to draw attention
  const badge = $('notification-badge');
  if (badge) badgePulse(badge);
}

// ── Drawer ───────────────────────────────────────────────────────────────

/** Toggle drawer visibility. */
export function toggleDrawer() {
  _isOpen = !_isOpen;
  const drawer = $('notification-drawer');
  if (!drawer) return;
  drawer.style.display = _isOpen ? 'flex' : 'none';
  if (_isOpen) renderList();
}

/** Close drawer. */
export function closeDrawer() {
  _isOpen = false;
  const drawer = $('notification-drawer');
  if (drawer) drawer.style.display = 'none';
}

// ── Bulk actions ─────────────────────────────────────────────────────────

/** Clear all notifications. */
export function clearAll() {
  _notifications = [];
  updateBadge();
  persist();
  renderList();
}

/** Mark one notification read, update badge. */
export function markOneRead(id: string) {
  _notifications = markRead(_notifications, id);
  updateBadge();
  persist();
  if (_isOpen) renderList();
}

/** Mark all read. */
export function markAllAsRead() {
  _notifications = markAllRead(_notifications);
  updateBadge();
  persist();
  if (_isOpen) renderList();
}

// ── Badge ────────────────────────────────────────────────────────────────

function updateBadge() {
  const badge = $('notification-badge');
  if (!badge) return;
  const count = countUnread(_notifications);
  badge.textContent = String(count);
  badge.style.display = count > 0 ? 'inline-flex' : 'none';
}

// ── Navigation ───────────────────────────────────────────────────────────

function navigateToView(viewName: string): void {
  // Dynamic import to avoid circular deps — router is at the view layer
  import('../../views/router').then(({ switchView }) => {
    switchView(viewName);
    closeDrawer();
  });
}

// ── Render ────────────────────────────────────────────────────────────────

function renderList() {
  const list = $('notification-drawer-list');
  if (!list) return;

  if (_notifications.length === 0) {
    list.innerHTML = `<div class="notification-empty">No notifications</div>`;
    return;
  }

  list.innerHTML = _notifications
    .map((n) => {
      const iconName = notificationIcon(n.kind);
      const time = relativeTime(n.timestamp);
      const unreadClass = n.read ? '' : ' unread';
      const clickable = n.navigateTo ? ' clickable' : '';
      const agentTag = n.agent ? `<span class="notification-agent">${escHtml(n.agent)}</span>` : '';
      const bodyHtml = n.body ? `<span class="notification-body">${escHtml(n.body)}</span>` : '';
      return `<div class="notification-item${unreadClass}${clickable}" data-notif-id="${n.id}" ${n.navigateTo ? `data-navigate="${escHtml(n.navigateTo)}"` : ''}>
        <span class="notification-icon"><span class="ms ms-sm">${iconName}</span></span>
        <div class="notification-content">
          <span class="notification-title">${escHtml(n.title)}</span>
          ${bodyHtml}
          ${agentTag}
        </div>
        <span class="notification-time">${time}</span>
      </div>`;
    })
    .join('');

  // Click handler: mark read + navigate
  list.querySelectorAll('.notification-item').forEach((el) => {
    el.addEventListener('click', () => {
      const id = el.getAttribute('data-notif-id');
      if (id) markOneRead(id);
      const nav = el.getAttribute('data-navigate');
      if (nav) navigateToView(nav);
    });
  });
}

// ── Init ─────────────────────────────────────────────────────────────────

/** Initialise notification bell click + clear + mark-all-read buttons; load persisted state. */
export function initNotifications() {
  // Restore persisted notifications
  _notifications = loadFromStorage();
  updateBadge();

  $('notification-bell')?.addEventListener('click', (e) => {
    e.stopPropagation();
    toggleDrawer();
  });
  $('notification-clear-btn')?.addEventListener('click', () => {
    clearAll();
  });
  $('notification-mark-read-btn')?.addEventListener('click', () => {
    markAllAsRead();
  });
  // Close drawer when clicking outside
  document.addEventListener('click', (e) => {
    if (!_isOpen) return;
    const drawer = $('notification-drawer');
    const bell = $('notification-bell');
    if (drawer && !drawer.contains(e.target as Node) && bell && !bell.contains(e.target as Node)) {
      closeDrawer();
    }
  });
}
