// src/components/molecules/markdown.ts
// Rich markdown renderer for chat messages (supports fenced code blocks).
// Views that only need simple bold/italic can use the lighter formatMarkdown
// already in components/helpers.ts.

export function escHtml(s: string): string {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}

export function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// Monotonic counter avoids Date.now() collisions when called rapidly (streaming)
let _placeholderCounter = 0;

/** Full markdown-to-HTML renderer used for chat message bubbles. */
export function formatMarkdown(text: string): string {
  if (!text) return '';

  // 1. Extract fenced code blocks into placeholders.
  //    Use a monotonic counter (not Date.now()) to guarantee uniqueness even
  //    when called multiple times in the same millisecond during streaming.
  //    The prefix uses only ASCII letters/digits so escHtml is a no-op on it.
  //    We also verify the prefix doesn't collide with the input text.
  const codeBlocks: string[] = [];
  let pfx: string;
  do {
    pfx = `PAWZCB${++_placeholderCounter}X`;
  } while (text.includes(pfx));

  // Match complete fenced code blocks: ```lang\n...```
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    const idx = codeBlocks.length;
    codeBlocks.push(
      `<pre class="code-block" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`,
    );
    return `${pfx}${idx}${pfx}`;
  });

  // Also handle incomplete fenced code blocks during streaming:
  // An opening ``` with no closing ``` should still render as a code block
  // rather than being mangled by the inline markdown transforms.
  html = html.replace(/```(\w*)\n([\s\S]+)$/g, (_m, lang, code) => {
    const idx = codeBlocks.length;
    codeBlocks.push(
      `<pre class="code-block streaming" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`,
    );
    return `${pfx}${idx}${pfx}`;
  });

  // 2. Escape the entire remaining text (all user content is now safe).
  //    Because the placeholder prefix is pure ASCII alphanumeric, escHtml
  //    passes it through unchanged — no mismatch on restoration.
  html = escHtml(html);

  // 3. Apply inline markdown transforms on the escaped text.
  //    Order matters: process multi-char markers before single-char ones.

  //    :icon_name: → Material Symbol (only known-safe ligature chars a-z_0-9)
  html = html.replace(/:([a-z][a-z0-9_]{1,30}):/g, '<span class="ms ms-sm">$1</span>');

  //    Inline code (must come before bold/italic to avoid conflicts)
  html = html.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');

  //    Bold and italic — use non-greedy matching with word-boundary awareness
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/(^|[^*])\*([^*]+)\*(?!\*)/g, '$1<em>$2</em>');

  //    Headings (only at line start)
  html = html.replace(/^### (.+)$/gm, '<h4>$1</h4>');
  html = html.replace(/^## (.+)$/gm, '<h3>$1</h3>');
  html = html.replace(/^# (.+)$/gm, '<h2>$1</h2>');

  //    Horizontal rules
  html = html.replace(/^---$/gm, '<hr>');

  //    Bullet and numbered lists
  html = html.replace(/^[-•] (.+)$/gm, '<div class="md-bullet">• $1</div>');
  html = html.replace(/^\d+\. (.+)$/gm, '<div class="md-bullet">$&</div>');

  //    Markdown tables → simple HTML tables
  html = renderTables(html);

  //    Links
  html = html.replace(
    /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g,
    (_m, label, url) =>
      `<a href="${escAttr(url)}" target="_blank" rel="noopener">${escHtml(label)}</a>`,
  );

  //    Newlines → <br> (after all block-level transforms)
  html = html.replace(/\n/g, '<br>');

  // 4. Restore code block placeholders with the real rendered HTML.
  //    The placeholder survives escHtml unchanged (pure alphanumeric).
  for (let i = 0; i < codeBlocks.length; i++) {
    html = html.split(`${pfx}${i}${pfx}`).join(codeBlocks[i]);
  }

  return html;
}

// ── Table rendering ──────────────────────────────────────────────────────

/**
 * Convert markdown tables to HTML tables.
 * Handles standard GFM pipe tables with header separator rows.
 */
function renderTables(html: string): string {
  // Match table blocks: header row, separator row, then body rows
  return html.replace(
    /^(\|.+\|)\n(\|[-| :]+\|)\n((?:\|.+\|\n?)+)/gm,
    (_match, headerRow: string, _sep: string, bodyBlock: string) => {
      const headers = headerRow
        .split('|')
        .filter((c: string) => c.trim() !== '')
        .map((c: string) => `<th>${c.trim()}</th>`)
        .join('');
      const rows = bodyBlock
        .trim()
        .split('\n')
        .map((row: string) => {
          const cells = row
            .split('|')
            .filter((c: string) => c.trim() !== '')
            .map((c: string) => `<td>${c.trim()}</td>`)
            .join('');
          return `<tr>${cells}</tr>`;
        })
        .join('');
      return `<table class="md-table"><thead><tr>${headers}</tr></thead><tbody>${rows}</tbody></table>`;
    },
  );
}
