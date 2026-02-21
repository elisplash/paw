// Automations / Cron View — Pure helpers (no DOM, no IPC)

export const MORNING_BRIEF_PROMPT = `Good morning! Please give me a concise daily briefing that includes:

1. **Weather** — Current conditions and today's forecast for my location
2. **Calendar** — Any scheduled events or deadlines today
3. **Tasks** — My top priority tasks and anything overdue
4. **News** — 3-5 key headlines relevant to my interests
5. **Memories** — Any important context from recent conversations

Keep it brief and actionable. End with a motivational note to start the day.`;

export function isValidSchedule(s: string): boolean {
  const lower = s.trim().toLowerCase();
  if (lower.startsWith('every ')) {
    const rest = lower.slice(6).trim();
    if (/^\d+m$/.test(rest) || /^\d+h$/.test(rest)) return true;
  }
  if (lower.startsWith('daily ')) {
    const time = lower.slice(6).trim();
    if (/^\d{2}:\d{2}$/.test(time)) return true;
  }
  return false;
}
