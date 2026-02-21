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
  // 1. Extract fenced code blocks first (preserve them verbatim)
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    return `<pre class="code-block" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`;
  });

  // 2. Split on code blocks so we don't accidentally process their contents
  const parts = html.split(/(<pre class="code-block"[\s\S]*?<\/pre>)/);
  html = parts
    .map((part, i) => {
      if (i % 2 === 1) return part; // code block — leave as-is
      return escHtml(part)
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
          (_m, label, url) =>
            `<a href="${escAttr(url)}" target="_blank" rel="noopener">${label}</a>`,
        )
        .replace(/\n/g, '<br>');
    })
    .join('');

  return html;
}
