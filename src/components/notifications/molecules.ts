// molecules.ts — Notifications drawer rendering and state management

import { escHtml } from '../helpers';
import { relativeTime } from '../../views/today/atoms';
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

let _notifications: Notification[] = [];
let _isOpen = false;

/** Add a new notification. */
export function pushNotification(
  kind: NotificationKind,
  title: string,
  body?: string,
  agent?: string,
) {
  _notifications.unshift({
    id: createNotificationId(),
    kind,
    title,
    body,
    agent,
    timestamp: new Date().toISOString(),
    read: false,
  });
  // Cap at 50
  if (_notifications.length > 50) _notifications = _notifications.slice(0, 50);
  updateBadge();
  if (_isOpen) renderList();
}

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

/** Clear all notifications. */
export function clearAll() {
  _notifications = [];
  updateBadge();
  renderList();
}

/** Mark one notification read, update badge. */
export function markOneRead(id: string) {
  _notifications = markRead(_notifications, id);
  updateBadge();
  if (_isOpen) renderList();
}

/** Mark all read. */
export function markAllAsRead() {
  _notifications = markAllRead(_notifications);
  updateBadge();
  if (_isOpen) renderList();
}

function updateBadge() {
  const badge = $('notification-badge');
  if (!badge) return;
  const count = countUnread(_notifications);
  badge.textContent = String(count);
  badge.style.display = count > 0 ? 'inline-flex' : 'none';
}

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
      const agentTag = n.agent ? `<span class="notification-agent">${escHtml(n.agent)}</span>` : '';
      const bodyHtml = n.body ? `<span class="notification-body">${escHtml(n.body)}</span>` : '';
      return `<div class="notification-item${unreadClass}" data-notif-id="${n.id}">
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

  // Mark as read on click
  list.querySelectorAll('.notification-item').forEach((el) => {
    el.addEventListener('click', () => {
      const id = el.getAttribute('data-notif-id');
      if (id) markOneRead(id);
    });
  });
}

/** Initialise notification bell click + clear button. */
export function initNotifications() {
  $('notification-bell')?.addEventListener('click', (e) => {
    e.stopPropagation();
    toggleDrawer();
  });
  $('notification-clear-btn')?.addEventListener('click', () => {
    clearAll();
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
