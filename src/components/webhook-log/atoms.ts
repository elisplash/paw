// atoms.ts — Pure types and helpers for the webhook event log
// NO DOM, NO side effects

export interface WebhookLogEntry {
  peer: string;
  agentId: string;
  userId: string;
  messagePreview: string;
  timestamp: string;
}

/** Parse a Tauri event payload into a WebhookLogEntry. */
export function parseWebhookEvent(payload: Record<string, unknown>): WebhookLogEntry {
  return {
    peer: String(payload.peer ?? ''),
    agentId: String(payload.agent_id ?? ''),
    userId: String(payload.user_id ?? ''),
    messagePreview: String(payload.message_preview ?? ''),
    timestamp: String(payload.timestamp ?? new Date().toISOString()),
  };
}

/** Truncate a string for display. */
export function truncatePreview(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return `${text.slice(0, maxLen)}…`;
}
