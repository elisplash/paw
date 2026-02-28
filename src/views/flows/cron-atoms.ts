// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Cron / Schedule Atoms
// Cron parsing, validation, presets, and next-fire calculation.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

// ── Schedule Types ─────────────────────────────────────────────────────────

/** A registered flow schedule (trigger node with cron expression). */
export interface FlowSchedule {
  flowId: string;
  flowName: string;
  nodeId: string;
  schedule: string;
  enabled: boolean;
  lastFiredAt: number | null;
  nextFireAt: number | null;
}

/** Log entry for a schedule fire event. */
export interface ScheduleFireLog {
  flowId: string;
  flowName: string;
  firedAt: number;
  status: 'success' | 'error';
  error?: string;
}

// ── Cron Presets ───────────────────────────────────────────────────────────

/**
 * Common cron presets for the UI picker.
 */
export const CRON_PRESETS: Array<{ label: string; value: string; description: string }> = [
  { label: 'Every minute', value: '* * * * *', description: 'Runs every 60 seconds' },
  { label: 'Every 5 minutes', value: '*/5 * * * *', description: 'Runs every 5 minutes' },
  { label: 'Every 15 minutes', value: '*/15 * * * *', description: 'Runs every 15 minutes' },
  { label: 'Every hour', value: '0 * * * *', description: 'Runs at the start of every hour' },
  { label: 'Every 6 hours', value: '0 */6 * * *', description: 'Runs every 6 hours' },
  { label: 'Daily at midnight', value: '0 0 * * *', description: 'Runs once a day at 00:00' },
  { label: 'Daily at 9 AM', value: '0 9 * * *', description: 'Runs once a day at 09:00' },
  { label: 'Weekdays at 9 AM', value: '0 9 * * 1-5', description: 'Mon–Fri at 09:00' },
  { label: 'Every Monday', value: '0 9 * * 1', description: 'Every Monday at 09:00' },
  { label: 'Monthly (1st)', value: '0 0 1 * *', description: 'First day of every month at 00:00' },
];

// ── Cron Engine ────────────────────────────────────────────────────────────

/**
 * Parse a simple cron expression (minute hour dom month dow)
 * and determine the next fire time from a given reference.
 * Supports: *, N, * /N (step), N-M (range), N,M (list)
 * Returns null if the expression is invalid.
 */
export function nextCronFire(expression: string, from: Date = new Date()): Date | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return null;

  function matches(field: string, value: number, max: number): boolean {
    if (field === '*') return true;
    // Step: */N
    if (field.startsWith('*/')) {
      const step = parseInt(field.slice(2), 10);
      return !isNaN(step) && step > 0 && value % step === 0;
    }
    // Range: N-M
    if (field.includes('-') && !field.includes(',')) {
      const [lo, hi] = field.split('-').map(Number);
      if (isNaN(lo) || isNaN(hi)) return false;
      return value >= lo && value <= hi;
    }
    // List: N,M,...
    if (field.includes(',')) {
      return field.split(',').map(Number).includes(value);
    }
    // Exact
    const exact = parseInt(field, 10);
    return !isNaN(exact) && exact >= 0 && exact <= max && value === exact;
  }

  // Search forward up to 1 year (525600 minutes)
  const candidate = new Date(from.getTime());
  candidate.setSeconds(0, 0);
  candidate.setMinutes(candidate.getMinutes() + 1);

  for (let i = 0; i < 525600; i++) {
    const min = candidate.getMinutes();
    const hour = candidate.getHours();
    const dom = candidate.getDate();
    const month = candidate.getMonth() + 1; // 1-indexed
    const dow = candidate.getDay(); // 0=Sun

    if (
      matches(parts[0], min, 59) &&
      matches(parts[1], hour, 23) &&
      matches(parts[2], dom, 31) &&
      matches(parts[3], month, 12) &&
      matches(parts[4], dow, 7)
    ) {
      return candidate;
    }

    candidate.setMinutes(candidate.getMinutes() + 1);
  }

  return null; // No match within a year
}

/**
 * Validate a cron expression. Returns null if valid, or an error message.
 */
export function validateCron(expression: string): string | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return 'Expected 5 fields: minute hour day month weekday';
  const valid = /^(\*|\d+(-\d+)?(,\d+(-\d+)?)*|\*\/\d+)$/;
  const labels = ['minute', 'hour', 'day', 'month', 'weekday'];
  for (let i = 0; i < 5; i++) {
    if (!valid.test(parts[i])) return `Invalid ${labels[i]} field: "${parts[i]}"`;
  }
  return null;
}

/**
 * Human-readable description of a cron expression.
 */
export function describeCron(expression: string): string {
  const preset = CRON_PRESETS.find((p) => p.value === expression);
  if (preset) return preset.description;
  return `Schedule: ${expression}`;
}
