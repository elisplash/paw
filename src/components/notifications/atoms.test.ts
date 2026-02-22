import { describe, it, expect } from 'vitest';
import {
  notificationIcon,
  countUnread,
  markRead,
  markAllRead,
  createNotificationId,
  type Notification,
} from './atoms';

const makeNotif = (overrides: Partial<Notification> = {}): Notification => ({
  id: 'n1',
  kind: 'task',
  title: 'Test',
  timestamp: new Date().toISOString(),
  read: false,
  ...overrides,
});

describe('notificationIcon', () => {
  it('returns task_alt for task', () => {
    expect(notificationIcon('task')).toBe('task_alt');
  });

  it('returns gavel for hil', () => {
    expect(notificationIcon('hil')).toBe('gavel');
  });

  it('returns webhook for webhook', () => {
    expect(notificationIcon('webhook')).toBe('webhook');
  });

  it('returns info for system', () => {
    expect(notificationIcon('system')).toBe('info');
  });
});

describe('countUnread', () => {
  it('returns 0 for empty array', () => {
    expect(countUnread([])).toBe(0);
  });

  it('counts only unread notifications', () => {
    const notifs = [
      makeNotif({ id: '1', read: false }),
      makeNotif({ id: '2', read: true }),
      makeNotif({ id: '3', read: false }),
    ];
    expect(countUnread(notifs)).toBe(2);
  });

  it('returns 0 when all read', () => {
    const notifs = [makeNotif({ id: '1', read: true }), makeNotif({ id: '2', read: true })];
    expect(countUnread(notifs)).toBe(0);
  });
});

describe('markRead', () => {
  it('marks specific notification as read', () => {
    const notifs = [makeNotif({ id: '1', read: false }), makeNotif({ id: '2', read: false })];
    const result = markRead(notifs, '1');
    expect(result[0].read).toBe(true);
    expect(result[1].read).toBe(false);
  });

  it('does not mutate original array', () => {
    const notifs = [makeNotif({ id: '1', read: false })];
    const result = markRead(notifs, '1');
    expect(notifs[0].read).toBe(false);
    expect(result[0].read).toBe(true);
  });

  it('returns unchanged when id not found', () => {
    const notifs = [makeNotif({ id: '1', read: false })];
    const result = markRead(notifs, 'nonexistent');
    expect(result[0].read).toBe(false);
  });
});

describe('markAllRead', () => {
  it('marks all as read', () => {
    const notifs = [makeNotif({ id: '1', read: false }), makeNotif({ id: '2', read: false })];
    const result = markAllRead(notifs);
    expect(result.every((n) => n.read)).toBe(true);
  });

  it('does not mutate original', () => {
    const notifs = [makeNotif({ id: '1', read: false })];
    markAllRead(notifs);
    expect(notifs[0].read).toBe(false);
  });
});

describe('createNotificationId', () => {
  it('returns a string starting with notif-', () => {
    expect(createNotificationId()).toMatch(/^notif-\d+-[a-z0-9]+$/);
  });

  it('generates unique ids', () => {
    const ids = new Set(Array.from({ length: 20 }, () => createNotificationId()));
    expect(ids.size).toBe(20);
  });
});
