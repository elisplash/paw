import { describe, it, expect } from 'vitest';
import { parseWebhookEvent, truncatePreview } from './atoms';

describe('parseWebhookEvent', () => {
  it('parses a complete payload', () => {
    const entry = parseWebhookEvent({
      peer: '127.0.0.1',
      agent_id: 'agent-1',
      user_id: 'user-1',
      message_preview: 'Hello world',
      timestamp: '2025-01-01T00:00:00Z',
    });
    expect(entry.peer).toBe('127.0.0.1');
    expect(entry.agentId).toBe('agent-1');
    expect(entry.userId).toBe('user-1');
    expect(entry.messagePreview).toBe('Hello world');
    expect(entry.timestamp).toBe('2025-01-01T00:00:00Z');
  });

  it('handles missing fields gracefully', () => {
    const entry = parseWebhookEvent({});
    expect(entry.peer).toBe('');
    expect(entry.agentId).toBe('');
    expect(entry.userId).toBe('');
    expect(entry.messagePreview).toBe('');
    expect(entry.timestamp).toBeTruthy(); // fallback to now
  });

  it('coerces non-string values', () => {
    const entry = parseWebhookEvent({
      peer: 123,
      agent_id: null,
      user_id: undefined,
      message_preview: true,
    });
    expect(entry.peer).toBe('123');
    expect(entry.agentId).toBe(''); // null ?? '' → ''
    expect(entry.userId).toBe(''); // undefined ?? '' → ''
    expect(entry.messagePreview).toBe('true');
  });
});

describe('truncatePreview', () => {
  it('returns short text unchanged', () => {
    expect(truncatePreview('hi', 10)).toBe('hi');
  });

  it('truncates long text with ellipsis', () => {
    expect(truncatePreview('hello world foo', 5)).toBe('hello…');
  });

  it('returns text at exact limit without ellipsis', () => {
    expect(truncatePreview('exact', 5)).toBe('exact');
  });
});
