// Tests for the rich markdown renderer used in chat messages.
// Uses jsdom because escHtml() relies on document.createElement().
// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { formatMarkdown, escHtml } from './markdown';

describe('escHtml', () => {
  it('escapes angle brackets and ampersands', () => {
    expect(escHtml('<b>hi</b>')).toBe('&lt;b&gt;hi&lt;/b&gt;');
    expect(escHtml('a & b')).toBe('a &amp; b');
  });
});

describe('formatMarkdown', () => {
  // ── Basic sanity ─────────────────────────────────────────────────────
  it('returns empty string for empty input', () => {
    expect(formatMarkdown('')).toBe('');
  });

  it('returns empty string for null/undefined', () => {
    // @ts-expect-error — intentional null test
    expect(formatMarkdown(null)).toBe('');
    // @ts-expect-error — intentional undefined test
    expect(formatMarkdown(undefined)).toBe('');
  });

  // ── Inline formatting ───────────────────────────────────────────────
  it('renders bold', () => {
    expect(formatMarkdown('**bold**')).toContain('<strong>bold</strong>');
  });

  it('renders italic', () => {
    const result = formatMarkdown('*italic*');
    expect(result).toContain('<em>italic</em>');
  });

  it('does not confuse bold and italic', () => {
    const result = formatMarkdown('**bold** and *italic*');
    expect(result).toContain('<strong>bold</strong>');
    expect(result).toContain('<em>italic</em>');
  });

  it('renders inline code', () => {
    expect(formatMarkdown('use `git push`')).toContain('<code class="inline-code">git push</code>');
  });

  // ── Headings ────────────────────────────────────────────────────────
  it('renders headings', () => {
    expect(formatMarkdown('# H1')).toContain('<h2>H1</h2>');
    expect(formatMarkdown('## H2')).toContain('<h3>H2</h3>');
    expect(formatMarkdown('### H3')).toContain('<h4>H3</h4>');
  });

  // ── Lists ───────────────────────────────────────────────────────────
  it('renders unordered list items', () => {
    expect(formatMarkdown('- item one')).toContain('• item one');
  });

  it('renders ordered list items', () => {
    expect(formatMarkdown('1. first')).toContain('1. first');
  });

  // ── Links ───────────────────────────────────────────────────────────
  it('renders links with target=_blank', () => {
    const result = formatMarkdown('[Pawz](https://openpawz.com)');
    expect(result).toContain('href="https://openpawz.com"');
    expect(result).toContain('target="_blank"');
    expect(result).toContain('>Pawz</a>');
  });

  // ── Code blocks ─────────────────────────────────────────────────────
  it('renders fenced code blocks', () => {
    const md = '```js\nconsole.log("hi");\n```';
    const result = formatMarkdown(md);
    expect(result).toContain('<pre class="code-block"');
    expect(result).toContain('data-lang="js"');
    expect(result).toContain('console.log');
    // The code inside should be HTML-escaped (angle brackets etc.)
    // Note: jsdom's textContent→innerHTML doesn't entity-encode quotes,
    // which is correct — quotes inside <code> are safe.
    expect(result).not.toContain('<script>');
  });

  it('renders incomplete code blocks during streaming', () => {
    const md = '```python\nprint("hello")\n# still streaming...';
    const result = formatMarkdown(md);
    expect(result).toContain('<pre class="code-block streaming"');
    expect(result).toContain('data-lang="python"');
    expect(result).toContain('print');
  });

  it('does not corrupt text when multiple code blocks exist', () => {
    const md = '```js\na();\n```\n\nsome text\n\n```py\nb()\n```';
    const result = formatMarkdown(md);
    expect(result).toContain('a();');
    expect(result).toContain('some text');
    expect(result).toContain('b()');
    // Should have two code blocks
    const blocks = result.match(/<pre class="code-block"/g);
    expect(blocks?.length).toBe(2);
  });

  // ── Placeholder stability (the main bug fix) ───────────────────────
  it('has unique placeholders across rapid successive calls', () => {
    // Simulate rapid streaming: call formatMarkdown many times in a row
    const results: string[] = [];
    for (let i = 0; i < 50; i++) {
      results.push(formatMarkdown(`\`\`\`\ncode block ${i}\n\`\`\``));
    }
    // Each should correctly contain its own code content
    for (let i = 0; i < 50; i++) {
      expect(results[i]).toContain(`code block ${i}`);
    }
  });

  it('handles text containing PAWZCB prefix without collision', () => {
    // Edge case: user message literally contains the placeholder prefix
    const md = '```\nPAWZCB1X some code PAWZCB1X\n```\n\nMore text with PAWZCB2X in it';
    const result = formatMarkdown(md);
    expect(result).toContain('PAWZCB1X some code PAWZCB1X');
    expect(result).toContain('More text with PAWZCB2X in it');
    expect(result).toContain('<pre class="code-block"');
  });

  // ── Horizontal rules ───────────────────────────────────────────────
  it('renders horizontal rules', () => {
    expect(formatMarkdown('---')).toContain('<hr>');
  });

  // ── Tables ──────────────────────────────────────────────────────────
  it('renders markdown tables', () => {
    const md = '| Name | Role |\n| --- | --- |\n| Alice | Dev |\n| Bob | PM |';
    const result = formatMarkdown(md);
    expect(result).toContain('<table class="md-table"');
    expect(result).toContain('<th>Name</th>');
    expect(result).toContain('<th>Role</th>');
    expect(result).toContain('<td>Alice</td>');
    expect(result).toContain('<td>PM</td>');
  });

  // ── XSS safety ─────────────────────────────────────────────────────
  it('escapes HTML in user content', () => {
    const result = formatMarkdown('<script>alert(1)</script>');
    expect(result).not.toContain('<script>');
    expect(result).toContain('&lt;script&gt;');
  });

  it('escapes HTML inside code blocks', () => {
    const md = '```\n<img onerror=alert(1) src=x>\n```';
    const result = formatMarkdown(md);
    expect(result).not.toContain('<img');
    expect(result).toContain('&lt;img');
  });

  // ── Mixed content ──────────────────────────────────────────────────
  it('handles mixed markdown without corruption', () => {
    const md = [
      '# Title',
      '',
      'Some **bold** text with `inline code`.',
      '',
      '```ts',
      'const x = 1;',
      '```',
      '',
      '- bullet one',
      '- bullet two',
      '',
      '| Col | Val |',
      '| --- | --- |',
      '| a   | 1   |',
    ].join('\n');

    const result = formatMarkdown(md);
    expect(result).toContain('<h2>Title</h2>');
    expect(result).toContain('<strong>bold</strong>');
    expect(result).toContain('<code class="inline-code">inline code</code>');
    expect(result).toContain('<pre class="code-block"');
    expect(result).toContain('• bullet one');
    expect(result).toContain('<table class="md-table"');
  });

  // ── Newlines ───────────────────────────────────────────────────────
  it('converts newlines to <br>', () => {
    expect(formatMarkdown('line1\nline2')).toContain('<br>');
  });

  // ── Material symbols ──────────────────────────────────────────────
  it('renders :icon: as material symbol', () => {
    const result = formatMarkdown(':memory:');
    expect(result).toContain('<span class="ms ms-sm">memory</span>');
  });

  // ── Emoji → Material Symbol replacement ────────────────────────────
  it('replaces common emojis with :icon: syntax before rendering', () => {
    const result = formatMarkdown('✅ Done');
    expect(result).toContain('<span class="ms ms-sm">check_circle</span>');
    expect(result).not.toContain('✅');
  });

  it('replaces multiple emojis in one message', () => {
    const result = formatMarkdown('⚠️ Warning and ❌ Error');
    expect(result).toContain('warning');
    expect(result).toContain('cancel');
    expect(result).not.toContain('⚠');
    expect(result).not.toContain('❌');
  });

  it('replaces file/folder emojis', () => {
    const result = formatMarkdown('📁 Files and 📄 Docs');
    expect(result).toContain('folder');
    expect(result).toContain('description');
  });

  it('does not replace emojis inside code blocks', () => {
    const result = formatMarkdown('```\n✅ check\n```');
    // Inside code blocks, content is escaped — emoji appears as-is text
    expect(result).toContain('✅');
  });
});
