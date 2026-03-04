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

/** Full markdown-to-HTML renderer used for chat message bubbles. */
export function formatMarkdown(text: string): string {
  // 1. Extract fenced code blocks into placeholders (unique tokens, NOT real HTML)
  //    This prevents an attacker from injecting a fake <pre class="code-block"> tag
  //    in the raw input that would bypass escHtml during the split step.
  const codeBlocks: string[] = [];
  const PLACEHOLDER_PREFIX = `\x00CB${Date.now()}\x00`; // unique, cannot appear in user text
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    const idx = codeBlocks.length;
    codeBlocks.push(
      `<pre class="code-block" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`,
    );
    return `${PLACEHOLDER_PREFIX}${idx}${PLACEHOLDER_PREFIX}`;
  });

  // 2. Escape the entire remaining text (all user content is now safe)
  html = escHtml(html);

  // 3. Apply inline markdown transforms on the escaped text
  //    :icon_name: → Material Symbol (only known-safe ligature chars a-z_0-9)
  html = html.replace(/:([a-z][a-z0-9_]{1,30}):/g, '<span class="ms ms-sm">$1</span>');

  html = html
    .replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>')
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.*?)\*/g, '<em>$1</em>')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/^[-•] (.+)$/gm, '<div class="md-bullet">• $1</div>')
    .replace(/^\d+\. (.+)$/gm, '<div class="md-bullet">$&</div>')
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g,
      (_m, label, url) => `<a href="${escAttr(url)}" target="_blank" rel="noopener">${label}</a>`,
    )
    .replace(/\n/g, '<br>');

  // 4. Restore code block placeholders with the real rendered HTML
  for (let i = 0; i < codeBlocks.length; i++) {
    // The placeholder was escaped by escHtml, so match the escaped version
    const escapedPlaceholder = escHtml(`${PLACEHOLDER_PREFIX}${i}${PLACEHOLDER_PREFIX}`);
    html = html.replace(escapedPlaceholder, codeBlocks[i]);
  }

  return html;
}
