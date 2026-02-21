// Tasks Hub — Atoms (pure data, zero DOM / zero IPC)

import type { TaskStatus } from '../../engine';

// ── Column definitions ─────────────────────────────────────────────────────

export const COLUMNS: TaskStatus[] = [
  'inbox',
  'assigned',
  'in_progress',
  'review',
  'blocked',
  'done',
];
