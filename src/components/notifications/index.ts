// index.ts — Notifications barrel + re-exports

export {
  initNotifications,
  pushNotification,
  clearAll,
  closeDrawer,
  markAllAsRead,
} from './molecules';
export type { Notification, NotificationKind } from './atoms';
