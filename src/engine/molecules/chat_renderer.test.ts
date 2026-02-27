// @vitest-environment jsdom
// src/engine/molecules/chat_renderer.test.ts
// Tests for the scoped message rendering molecule.

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the markdown formatter and icon helper to avoid pulling in full deps.
vi.mock('../../components/molecules/markdown', () => ({
  formatMarkdown: (text: string) => text,
}));
vi.mock('../../components/helpers', () => ({
  icon: (name: string) => `<svg data-icon="${name}"></svg>`,
  escHtml: (s: string) => s,
}));

import {
  renderSingleMessage,
  renderMessages,
  renderAttachmentStrip,
  showStreamingMessage,
  appendStreamingDelta,
  appendThinkingDelta,
  scrollToBottom,
} from './chat_renderer';

import type { Message } from '../../types';
import type { RenderOpts } from './chat_renderer';

// ── Helpers ──────────────────────────────────────────────────────────────

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    role: 'user',
    content: 'Hello',
    timestamp: new Date('2025-01-01T12:00:00Z'),
    ...overrides,
  };
}

const defaultOpts: RenderOpts = {
  agentName: 'Aria',
};

// ── renderSingleMessage ──────────────────────────────────────────────────

describe('renderSingleMessage', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
  });

  it('renders a user message with "YOU ›" prefix', () => {
    const msg = makeMessage({ role: 'user', content: 'Hi there' });
    const el = renderSingleMessage(container, msg, 0, 0, -1, defaultOpts);
    expect(el.classList.contains('message')).toBe(true);
    expect(el.classList.contains('user')).toBe(true);
    const prefix = el.querySelector('.message-prefix')!;
    expect(prefix.textContent).toBe('YOU ›');
  });

  it('renders an assistant message with agent name prefix', () => {
    const msg = makeMessage({ role: 'assistant', content: 'I can help' });
    const el = renderSingleMessage(container, msg, 0, -1, 0, defaultOpts);
    expect(el.classList.contains('assistant')).toBe(true);
    const prefix = el.querySelector('.message-prefix')!;
    expect(prefix.textContent).toBe('ARIA ›');
  });

  it('renders a system message with "SYS ›" prefix', () => {
    const msg = makeMessage({ role: 'system', content: 'System notice' });
    const el = renderSingleMessage(container, msg, 0, -1, -1, defaultOpts);
    const prefix = el.querySelector('.message-prefix')!;
    expect(prefix.textContent).toBe('SYS ›');
  });

  it('renders multi-agent prefix with colour from agentMap', () => {
    const agentMap = new Map([['bot-1', { name: 'Scout', color: '#ff0000' }]]);
    const msg = makeMessage({
      role: 'assistant',
      content: 'Reply from Scout',
      agentId: 'bot-1',
    });
    const el = renderSingleMessage(container, msg, 0, -1, 0, {
      ...defaultOpts,
      agentMap,
    });
    const prefix = el.querySelector('.message-prefix') as HTMLElement;
    expect(prefix.textContent).toBe('SCOUT ›');
    expect(prefix.style.color).toBe('rgb(255, 0, 0)');
  });

  it('renders timestamp', () => {
    const msg = makeMessage();
    const el = renderSingleMessage(container, msg, 0, 0, -1, defaultOpts);
    const time = el.querySelector('.message-time')!;
    expect(time.textContent).toBeTruthy();
  });

  it('renders thinking block when thinkingContent present', () => {
    const msg = makeMessage({
      role: 'assistant',
      content: 'Answer',
      thinkingContent: 'Let me think…',
    });
    const el = renderSingleMessage(container, msg, 0, -1, 0, defaultOpts);
    const thinking = el.querySelector('.thinking-block');
    expect(thinking).not.toBeNull();
    expect(thinking!.querySelector('.thinking-content')!.innerHTML).toContain('Let me think');
  });

  it('renders tool calls badge', () => {
    const msg = makeMessage({
      role: 'assistant',
      content: 'Done',
      toolCalls: [{ name: 'test', input: '{}', result: 'ok' }] as any,
    });
    const el = renderSingleMessage(container, msg, 0, -1, 0, defaultOpts);
    const badge = el.querySelector('.tool-calls-badge');
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toContain('1 tool call');
  });

  it('renders retry button on last user message', () => {
    const onRetry = vi.fn();
    const msg = makeMessage({ role: 'user', content: 'Retry me' });
    const el = renderSingleMessage(container, msg, 0, 0, -1, {
      ...defaultOpts,
      onRetry,
    });
    const btn = el.querySelector('.message-retry-btn') as HTMLButtonElement;
    expect(btn).not.toBeNull();
    btn.click();
    expect(onRetry).toHaveBeenCalledWith('Retry me');
  });

  it('does not render retry during streaming', () => {
    const msg = makeMessage({ role: 'user', content: 'Hello' });
    const el = renderSingleMessage(container, msg, 0, 0, -1, {
      ...defaultOpts,
      onRetry: vi.fn(),
      isStreaming: true,
    });
    expect(el.querySelector('.message-retry-btn')).toBeNull();
  });

  it('renders TTS button on assistant messages', () => {
    const onSpeak = vi.fn();
    const msg = makeMessage({ role: 'assistant', content: 'Say this' });
    const el = renderSingleMessage(container, msg, 0, -1, 0, {
      ...defaultOpts,
      onSpeak,
    });
    expect(el.querySelector('.message-tts-btn')).not.toBeNull();
  });

  it('does not render TTS button on error messages', () => {
    const msg = makeMessage({ role: 'assistant', content: 'Error: something went wrong' });
    const el = renderSingleMessage(container, msg, 0, -1, 0, {
      ...defaultOpts,
      onSpeak: vi.fn(),
    });
    expect(el.querySelector('.message-tts-btn')).toBeNull();
  });

  it('renders attachments when present', () => {
    const msg = makeMessage({
      attachments: [{ name: 'doc.pdf', mimeType: 'application/pdf' }],
    });
    const el = renderSingleMessage(container, msg, 0, 0, -1, defaultOpts);
    expect(el.querySelector('.message-attachments')).not.toBeNull();
  });
});

// ── renderMessages ───────────────────────────────────────────────────────

describe('renderMessages', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
  });

  it('renders all messages into the container', () => {
    const msgs = [
      makeMessage({ role: 'user', content: 'Q1' }),
      makeMessage({ role: 'assistant', content: 'A1' }),
      makeMessage({ role: 'user', content: 'Q2' }),
    ];
    renderMessages(container, msgs, defaultOpts);
    expect(container.querySelectorAll('.message').length).toBe(3);
  });

  it('shows empty state when messages array is empty', () => {
    const emptyEl = document.createElement('div');
    emptyEl.style.display = 'none';
    renderMessages(container, [], defaultOpts, emptyEl);
    expect(emptyEl.style.display).toBe('flex');
  });

  it('hides empty state when messages are present', () => {
    const emptyEl = document.createElement('div');
    emptyEl.style.display = 'flex';
    renderMessages(container, [makeMessage()], defaultOpts, emptyEl);
    expect(emptyEl.style.display).toBe('none');
  });

  it('preserves streaming message on re-render', () => {
    const streamingEl = document.createElement('div');
    streamingEl.id = 'streaming-message';
    container.appendChild(streamingEl);

    renderMessages(container, [makeMessage()], defaultOpts);
    // Streaming message should still be in the container
    expect(container.querySelector('#streaming-message')).not.toBeNull();
  });

  it('clears previous messages on re-render', () => {
    renderMessages(container, [makeMessage()], defaultOpts);
    expect(container.querySelectorAll('.message').length).toBe(1);
    renderMessages(container, [makeMessage(), makeMessage()], defaultOpts);
    expect(container.querySelectorAll('.message').length).toBe(2);
  });
});

// ── renderAttachmentStrip ────────────────────────────────────────────────

describe('renderAttachmentStrip', () => {
  it('renders image attachments as cards', () => {
    const strip = renderAttachmentStrip([
      { name: 'photo.png', mimeType: 'image/png', url: 'http://example.com/photo.png' },
    ]);
    expect(strip.querySelector('.message-attachment-card')).not.toBeNull();
    expect(strip.querySelector('img')!.alt).toBe('photo.png');
  });

  it('renders non-image attachments as doc chips', () => {
    const strip = renderAttachmentStrip([{ name: 'readme.txt', mimeType: 'text/plain' }]);
    expect(strip.querySelector('.message-attachment-doc')).not.toBeNull();
    expect(strip.querySelector('span')!.textContent).toBe('readme.txt');
  });

  it('renders a mix of image and non-image attachments', () => {
    const strip = renderAttachmentStrip([
      { name: 'a.jpg', mimeType: 'image/jpeg', url: 'http://example.com/a.jpg' },
      { name: 'b.pdf', mimeType: 'application/pdf' },
    ]);
    expect(strip.querySelector('.message-attachment-card')).not.toBeNull();
    expect(strip.querySelector('.message-attachment-doc')).not.toBeNull();
  });

  it('uses base64 data for image src when url is absent', () => {
    const strip = renderAttachmentStrip([{ name: 'img.png', mimeType: 'image/png', data: 'AAAA' }]);
    const img = strip.querySelector('img')!;
    expect(img.src).toContain('data:image/png;base64,AAAA');
  });
});

// ── showStreamingMessage ─────────────────────────────────────────────────

describe('showStreamingMessage', () => {
  it('inserts a streaming message with loading dots', () => {
    const container = document.createElement('div');
    const contentEl = showStreamingMessage(container, 'Aria');
    const streamMsg = container.querySelector('#streaming-message');
    expect(streamMsg).not.toBeNull();
    expect(contentEl.querySelector('.loading-dots')).not.toBeNull();
  });

  it('uses the agent name in the prefix', () => {
    const container = document.createElement('div');
    showStreamingMessage(container, 'Scout');
    const prefix = container.querySelector('.message-prefix')!;
    expect(prefix.textContent).toBe('SCOUT ›');
  });
});

// ── appendStreamingDelta ─────────────────────────────────────────────────

describe('appendStreamingDelta', () => {
  it('updates the element innerHTML with formatted content', () => {
    const el = document.createElement('span');
    appendStreamingDelta(el, 'Hello world');
    expect(el.innerHTML).toBe('Hello world'); // mock formatMarkdown returns as-is
  });
});

// ── appendThinkingDelta ──────────────────────────────────────────────────

describe('appendThinkingDelta', () => {
  it('creates a thinking block details element', () => {
    const streamMsg = document.createElement('div');
    const content = document.createElement('div');
    content.className = 'message-content';
    streamMsg.appendChild(content);

    appendThinkingDelta(streamMsg, 'Reasoning…');
    const thinking = streamMsg.querySelector('.thinking-block')!;
    expect(thinking).not.toBeNull();
    expect(thinking.querySelector('.thinking-content')!.innerHTML).toContain('Reasoning');
  });

  it('reuses existing thinking block on subsequent calls', () => {
    const streamMsg = document.createElement('div');
    const content = document.createElement('div');
    content.className = 'message-content';
    streamMsg.appendChild(content);

    appendThinkingDelta(streamMsg, 'Step 1');
    appendThinkingDelta(streamMsg, 'Step 1\nStep 2');
    const blocks = streamMsg.querySelectorAll('.thinking-block');
    expect(blocks.length).toBe(1);
    expect(blocks[0].querySelector('.thinking-content')!.innerHTML).toContain('Step 2');
  });
});

// ── scrollToBottom ───────────────────────────────────────────────────────

describe('scrollToBottom', () => {
  it('sets scrollTop to scrollHeight via rAF', async () => {
    const container = document.createElement('div');
    Object.defineProperty(container, 'scrollHeight', { value: 500 });
    const rafRef = { value: false };

    // Mock requestAnimationFrame to run immediately
    const origRAF = globalThis.requestAnimationFrame;
    globalThis.requestAnimationFrame = (cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    };

    scrollToBottom(container, rafRef);
    expect(container.scrollTop).toBe(500);

    globalThis.requestAnimationFrame = origRAF;
  });

  it('debounces when rAF is already pending', () => {
    const container = document.createElement('div');
    const rafRef = { value: true };
    scrollToBottom(container, rafRef);
    // Should not throw, just skip
    expect(rafRef.value).toBe(true);
  });
});
