// atoms.ts — Pure types and helpers for the notifications center
// NO DOM, NO side effects

export type NotificationKind = 'task' | 'message' | 'channel' | 'webhook' | 'hil' | 'system';

export interface Notification {
  id: string;
  kind: NotificationKind;
  title: string;
  body?: string;
  agent?: string;
  timestamp: string;
  read: boolean;
}

/** Map notification kind to Material Symbol icon name. */
export function notificationIcon(kind: NotificationKind): string {
  const map: Record<NotificationKind, string> = {
    task: 'task_alt',
    message: 'chat_bubble',
    channel: 'forum',
    webhook: 'webhook',
    hil: 'gavel',
    system: 'info',
  };
  return map[kind];
}

/** Count unread notifications. */
export function countUnread(notifications: Notification[]): number {
  return notifications.filter((n) => !n.read).length;
}

/** Mark a notification as read (returns new array, does not mutate). */
export function markRead(notifications: Notification[], id: string): Notification[] {
  return notifications.map((n) => (n.id === id ? { ...n, read: true } : n));
}

/** Mark all notifications as read (returns new array). */
export function markAllRead(notifications: Notification[]): Notification[] {
  return notifications.map((n) => (n.read ? n : { ...n, read: true }));
}

/** Create a unique notification ID. */
export function createNotificationId(): string {
  return `notif-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}
