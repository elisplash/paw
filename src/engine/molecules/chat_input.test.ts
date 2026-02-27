// @vitest-environment jsdom
// src/engine/molecules/chat_input.test.ts
// Tests for the scoped chat input controller molecule.

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock helpers and features that reach into the wider codebase
vi.mock('../../components/helpers', () => ({
  icon: (name: string) => `<svg data-icon="${name}"></svg>`,
  escHtml: (s: string) => s,
}));
vi.mock('../../features/slash-commands', () => ({
  getAutocompleteSuggestions: (prefix: string) => {
    if (prefix === '/he') {
      return [{ command: '/help', description: 'Show help' }];
    }
    return [];
  },
}));

import { createChatInput, type ChatInputController } from './chat_input';

// ── Factory & lifecycle ──────────────────────────────────────────────────

describe('createChatInput', () => {
  let ctrl: ChatInputController;

  beforeEach(() => {
    ctrl = createChatInput();
  });

  it('returns a controller with an el property', () => {
    expect(ctrl.el).toBeInstanceOf(HTMLElement);
    expect(ctrl.el.className).toBe('chat-input-area');
  });

  it('contains a textarea', () => {
    const textarea = ctrl.el.querySelector('textarea');
    expect(textarea).not.toBeNull();
    expect(textarea!.className).toBe('chat-input');
  });

  it('contains a send button', () => {
    const sendBtn = ctrl.el.querySelector('.chat-send');
    expect(sendBtn).not.toBeNull();
  });

  it('contains an attach button by default', () => {
    expect(ctrl.el.querySelector('.chat-attach-btn')).not.toBeNull();
  });

  it('hides attach button when showAttachBtn is false', () => {
    const c = createChatInput({ showAttachBtn: false });
    expect(c.el.querySelector('.chat-attach-btn')).toBeNull();
  });

  it('contains a talk button by default', () => {
    expect(ctrl.el.querySelector('.chat-talk-btn')).not.toBeNull();
  });

  it('hides talk button when showTalkBtn is false', () => {
    const c = createChatInput({ showTalkBtn: false });
    expect(c.el.querySelector('.chat-talk-btn')).toBeNull();
  });

  it('uses custom placeholder', () => {
    const c = createChatInput({ placeholder: 'Type here…' });
    const ta = c.el.querySelector('textarea')!;
    expect(ta.placeholder).toBe('Type here…');
  });
});

// ── getValue / setValue / clear ───────────────────────────────────────────

describe('getValue / setValue / clear', () => {
  let ctrl: ChatInputController;

  beforeEach(() => {
    ctrl = createChatInput();
    document.body.appendChild(ctrl.el);
  });

  it('getValue returns trimmed textarea content', () => {
    ctrl.setValue('  hello  ');
    expect(ctrl.getValue()).toBe('hello');
  });

  it('setValue sets textarea value', () => {
    ctrl.setValue('test content');
    const ta = ctrl.el.querySelector('textarea')!;
    expect(ta.value).toBe('test content');
  });

  it('clear empties textarea and attachments', () => {
    ctrl.setValue('something');
    ctrl.setAttachments([new File(['data'], 'a.txt', { type: 'text/plain' })]);
    ctrl.clear();
    expect(ctrl.getValue()).toBe('');
    expect(ctrl.getAttachments()).toHaveLength(0);
  });
});

// ── Attachments ──────────────────────────────────────────────────────────

describe('attachments', () => {
  let ctrl: ChatInputController;

  beforeEach(() => {
    ctrl = createChatInput();
    document.body.appendChild(ctrl.el);
  });

  it('setAttachments stores files and getAttachments returns copies', () => {
    const file = new File(['x'], 'test.txt', { type: 'text/plain' });
    ctrl.setAttachments([file]);
    const result = ctrl.getAttachments();
    expect(result).toHaveLength(1);
    expect(result[0].name).toBe('test.txt');
  });

  it('clearAttachments removes all pending files', () => {
    ctrl.setAttachments([new File(['x'], 'a.txt')]);
    ctrl.clearAttachments();
    expect(ctrl.getAttachments()).toHaveLength(0);
  });

  it('setAttachments shows attachment preview strip', () => {
    ctrl.setAttachments([new File(['x'], 'a.txt', { type: 'text/plain' })]);
    const preview = ctrl.el.querySelector('.chat-attachment-preview') as HTMLElement;
    expect(preview.style.display).not.toBe('none');
    expect(preview.querySelectorAll('.attachment-chip').length).toBe(1);
  });

  it('clearAttachments hides preview strip', () => {
    ctrl.setAttachments([new File(['x'], 'a.txt')]);
    ctrl.clearAttachments();
    const preview = ctrl.el.querySelector('.chat-attachment-preview') as HTMLElement;
    expect(preview.style.display).toBe('none');
  });
});

// ── onSend callback ──────────────────────────────────────────────────────

describe('onSend', () => {
  let ctrl: ChatInputController;

  beforeEach(() => {
    ctrl = createChatInput();
    document.body.appendChild(ctrl.el);
  });

  it('fires onSend with content when send button is clicked', () => {
    const spy = vi.fn();
    ctrl.onSend = spy;
    ctrl.setValue('Hello');
    const sendBtn = ctrl.el.querySelector('.chat-send') as HTMLButtonElement;
    sendBtn.click();
    expect(spy).toHaveBeenCalledWith('Hello', []);
  });

  it('fires onSend with attachments', () => {
    const spy = vi.fn();
    ctrl.onSend = spy;
    const file = new File(['data'], 'img.png', { type: 'image/png' });
    ctrl.setAttachments([file]);
    ctrl.setValue('Look at this');
    const sendBtn = ctrl.el.querySelector('.chat-send') as HTMLButtonElement;
    sendBtn.click();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy.mock.calls[0][0]).toBe('Look at this');
    expect(spy.mock.calls[0][1]).toHaveLength(1);
    expect(spy.mock.calls[0][1][0].name).toBe('img.png');
  });

  it('does not fire onSend when textarea is empty', () => {
    const spy = vi.fn();
    ctrl.onSend = spy;
    const sendBtn = ctrl.el.querySelector('.chat-send') as HTMLButtonElement;
    sendBtn.click();
    expect(spy).not.toHaveBeenCalled();
  });

  it('fires onSend on Enter key (no shift)', () => {
    const spy = vi.fn();
    ctrl.onSend = spy;
    ctrl.setValue('Test');
    const textarea = ctrl.el.querySelector('textarea')!;
    textarea.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(spy).toHaveBeenCalledWith('Test', []);
  });

  it('does not fire onSend on Shift+Enter', () => {
    const spy = vi.fn();
    ctrl.onSend = spy;
    ctrl.setValue('Test');
    const textarea = ctrl.el.querySelector('textarea')!;
    textarea.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true }),
    );
    expect(spy).not.toHaveBeenCalled();
  });
});

// ── onTalk callback ──────────────────────────────────────────────────────

describe('onTalk', () => {
  it('fires onTalk when talk button is clicked', () => {
    const ctrl = createChatInput({ showTalkBtn: true });
    document.body.appendChild(ctrl.el);
    const spy = vi.fn();
    ctrl.onTalk = spy;
    const talkBtn = ctrl.el.querySelector('.chat-talk-btn') as HTMLButtonElement;
    talkBtn.click();
    expect(spy).toHaveBeenCalledTimes(1);
  });
});

// ── destroy ──────────────────────────────────────────────────────────────

describe('destroy', () => {
  it('removes the element from its parent', () => {
    const ctrl = createChatInput();
    const wrapper = document.createElement('div');
    wrapper.appendChild(ctrl.el);
    expect(wrapper.querySelector('.chat-input-area')).not.toBeNull();
    ctrl.destroy();
    expect(wrapper.querySelector('.chat-input-area')).toBeNull();
  });

  it('is idempotent — calling destroy twice does not throw', () => {
    const ctrl = createChatInput();
    const wrapper = document.createElement('div');
    wrapper.appendChild(ctrl.el);
    ctrl.destroy();
    expect(() => ctrl.destroy()).not.toThrow();
  });
});
