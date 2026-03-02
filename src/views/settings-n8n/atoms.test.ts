// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { esc, workflowCountLabel, makeBtn } from './atoms';

// ── esc ────────────────────────────────────────────────────────────────────

describe('esc (n8n)', () => {
  it('escapes HTML entities', () => {
    expect(esc('<script>"alert"</script>')).toBe('&lt;script&gt;&quot;alert&quot;&lt;/script&gt;');
  });

  it('escapes single quotes', () => {
    expect(esc("it's")).toBe('it&#39;s');
  });

  it('handles empty string', () => {
    expect(esc('')).toBe('');
  });

  it('handles ampersands', () => {
    expect(esc('A & B')).toBe('A &amp; B');
  });
});

// ── workflowCountLabel ─────────────────────────────────────────────────────

describe('workflowCountLabel', () => {
  it('returns singular for 1', () => {
    expect(workflowCountLabel(1)).toBe('1 workflow');
  });

  it('returns plural for 0', () => {
    expect(workflowCountLabel(0)).toBe('0 workflows');
  });

  it('returns plural for multiple', () => {
    expect(workflowCountLabel(5)).toBe('5 workflows');
    expect(workflowCountLabel(100)).toBe('100 workflows');
  });
});

// ── makeBtn ────────────────────────────────────────────────────────────────

describe('makeBtn', () => {
  it('creates a button element', () => {
    const btn = makeBtn('Click', 'btn-primary', () => {});
    expect(btn.tagName).toBe('BUTTON');
  });

  it('sets text content', () => {
    const btn = makeBtn('Save', 'btn-primary', () => {});
    expect(btn.textContent).toBe('Save');
  });

  it('sets className with btn prefix and btn-sm', () => {
    const btn = makeBtn('OK', 'btn-danger', () => {});
    expect(btn.className).toBe('btn btn-danger btn-sm');
  });

  it('attaches click handler', () => {
    const handler = vi.fn();
    const btn = makeBtn('Go', 'btn-primary', handler);
    btn.click();
    expect(handler).toHaveBeenCalledOnce();
  });
});

// ── Edge cases: esc ────────────────────────────────────────────────────

describe('esc (n8n) — edge cases', () => {
  it('escapes all 5 special chars combined', () => {
    expect(esc('<a href="x">&\'test\'</a>')).toBe(
      '&lt;a href=&quot;x&quot;&gt;&amp;&#39;test&#39;&lt;/a&gt;',
    );
  });

  it('passes through strings with no special chars', () => {
    expect(esc('Hello World 123')).toBe('Hello World 123');
  });

  it('handles unicode / emoji content', () => {
    expect(esc('Hello 🐾 <world>')).toBe('Hello 🐾 &lt;world&gt;');
  });

  it('handles very long string without corruption', () => {
    const long = 'x'.repeat(10_000) + '<>';
    const result = esc(long);
    expect(result).toContain('&lt;&gt;');
    expect(result.length).toBeGreaterThan(10_000);
  });
});

// ── Edge cases: workflowCountLabel ─────────────────────────────────────

describe('workflowCountLabel — edge cases', () => {
  it('handles negative numbers', () => {
    expect(workflowCountLabel(-1)).toBe('-1 workflows');
  });

  it('handles very large numbers', () => {
    expect(workflowCountLabel(Number.MAX_SAFE_INTEGER)).toBe(
      `${Number.MAX_SAFE_INTEGER} workflows`,
    );
  });

  it('formats decimal as-is (JavaScript coercion)', () => {
    expect(workflowCountLabel(1.5)).toBe('1.5 workflows');
  });
});

// ── Edge cases: makeBtn ────────────────────────────────────────────────

describe('makeBtn — edge cases', () => {
  it('handles empty label', () => {
    const btn = makeBtn('', 'btn-primary', () => {});
    expect(btn.textContent).toBe('');
  });

  it('handles empty class string', () => {
    const btn = makeBtn('OK', '', () => {});
    expect(btn.className).toBe('btn  btn-sm');
  });

  it('click handler can be called multiple times', () => {
    const handler = vi.fn();
    const btn = makeBtn('Go', 'btn-primary', handler);
    btn.click();
    btn.click();
    btn.click();
    expect(handler).toHaveBeenCalledTimes(3);
  });

  it('always includes btn-sm class', () => {
    const btn = makeBtn('X', 'btn-warning', () => {});
    expect(btn.className).toContain('btn-sm');
  });

  it('is an HTMLButtonElement instance', () => {
    const btn = makeBtn('A', 'b', () => {});
    expect(btn).toBeInstanceOf(HTMLButtonElement);
  });
});
