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
    const langLabel = lang ? `<span class="code-lang">${escHtml(lang)}</span>` : '';
    codeBlocks.push(
      `<div class="code-block-wrapper">${langLabel}<button class="code-copy-btn" title="Copy"><span class="ms" style="font-size:14px">content_copy</span></button><pre class="code-block" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre></div>`,
    );
    return `${pfx}${idx}${pfx}`;
  });

  // Also handle incomplete fenced code blocks during streaming:
  // An opening ``` with no closing ``` should still render as a code block
  // rather than being mangled by the inline markdown transforms.
  html = html.replace(/```(\w*)\n([\s\S]+)$/g, (_m, lang, code) => {
    const idx = codeBlocks.length;
    const langLabel = lang ? `<span class="code-lang">${escHtml(lang)}</span>` : '';
    codeBlocks.push(
      `<div class="code-block-wrapper streaming">${langLabel}<pre class="code-block streaming" data-lang="${escAttr(lang)}"><code>${escHtml(code.trimEnd())}</code></pre></div>`,
    );
    return `${pfx}${idx}${pfx}`;
  });

  // 1b. Replace common emoji/unicode symbols with :icon_name: syntax.
  //     Done AFTER code block extraction so emojis in code are preserved.
  html = replaceEmojis(html);

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

// ── Emoji → Material Symbol replacement ──────────────────────────────────

/** Map of common emoji/unicode chars → Material Symbol ligature names. */
const EMOJI_MAP: [RegExp, string][] = [
  [/✅/g, ':check_circle:'],
  [/❌/g, ':cancel:'],
  [/⚠️?/g, ':warning:'],
  [/ℹ️?/g, ':info:'],
  [/🔴/g, ':error:'],
  [/🟢/g, ':check_circle:'],
  [/🟡/g, ':warning:'],
  [/📁/g, ':folder:'],
  [/📂/g, ':folder_open:'],
  [/📄/g, ':description:'],
  [/📎/g, ':attach_file:'],
  [/🔗/g, ':link:'],
  [/🔧/g, ':build:'],
  [/🔨/g, ':build:'],
  [/🛠️?/g, ':build:'],
  [/⚙️?/g, ':settings:'],
  [/🚀/g, ':rocket_launch:'],
  [/💡/g, ':lightbulb:'],
  [/📝/g, ':edit_note:'],
  [/🗑️?/g, ':delete:'],
  [/📧/g, ':mail:'],
  [/📮/g, ':send:'],
  [/🔍/g, ':search:'],
  [/🔎/g, ':search:'],
  [/📊/g, ':bar_chart:'],
  [/📈/g, ':trending_up:'],
  [/📉/g, ':trending_down:'],
  [/⏰/g, ':schedule:'],
  [/🕐/g, ':schedule:'],
  [/⏱️?/g, ':timer:'],
  [/🎯/g, ':target:'],
  [/🏷️?/g, ':label:'],
  [/💰/g, ':attach_money:'],
  [/💵/g, ':attach_money:'],
  [/🔐/g, ':lock:'],
  [/🔑/g, ':key:'],
  [/🔒/g, ':lock:'],
  [/🔓/g, ':lock_open:'],
  [/👤/g, ':person:'],
  [/👥/g, ':group:'],
  [/🌐/g, ':language:'],
  [/💻/g, ':computer:'],
  [/🖥️?/g, ':desktop_windows:'],
  [/📱/g, ':smartphone:'],
  [/🎵/g, ':music_note:'],
  [/🎶/g, ':music_note:'],
  [/📸/g, ':photo_camera:'],
  [/🖼️?/g, ':image:'],
  [/✏️?/g, ':edit:'],
  [/📋/g, ':content_paste:'],
  [/▶️?/g, ':play_arrow:'],
  [/⏸️?/g, ':pause:'],
  [/⏹️?/g, ':stop:'],
  [/➡️?/g, ':arrow_forward:'],
  [/⬅️?/g, ':arrow_back:'],
  [/⬆️?/g, ':arrow_upward:'],
  [/⬇️?/g, ':arrow_downward:'],
  [/↗️?/g, ':north_east:'],
  [/✨/g, ':auto_awesome:'],
  [/🔥/g, ':local_fire_department:'],
  [/💬/g, ':chat:'],
  [/🗂️?/g, ':topic:'],
  [/📌/g, ':push_pin:'],
  [/🎉/g, ':celebration:'],
  [/👍/g, ':thumb_up:'],
  [/👎/g, ':thumb_down:'],
  [/⭐/g, ':star:'],
  [/🌟/g, ':star:'],
  [/🔄/g, ':sync:'],
  [/♻️?/g, ':recycling:'],
  [/🧪/g, ':science:'],
  [/🧩/g, ':extension:'],
  [/📦/g, ':inventory_2:'],
  [/🗓️?/g, ':calendar_today:'],
  [/📅/g, ':calendar_today:'],
  [/🏗️?/g, ':construction:'],
  [/🔔/g, ':notifications:'],
  [/🔕/g, ':notifications_off:'],
  // Checkmarks and arrows frequently used by models
  [/☑️?/g, ':check_box:'],
  [/✓/g, ':check:'],
  [/✔️?/g, ':check:'],
  [/•/g, '·'], // keep clean bullet
];

/** Replace emoji characters with :icon_name: syntax. */
function replaceEmojis(text: string): string {
  for (const [re, replacement] of EMOJI_MAP) {
    text = text.replace(re, replacement);
  }
  return text;
}

// ── Code block copy button wiring ────────────────────────────────────────

/**
 * Wire up all code-copy-btn buttons inside a container.
 * Call after setting innerHTML with formatMarkdown output.
 * Uses event delegation on the container.
 */
export function wireCodeCopyButtons(container: HTMLElement): void {
  container.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.code-copy-btn') as HTMLElement | null;
    if (!btn) return;
    const wrapper = btn.closest('.code-block-wrapper');
    const codeEl = wrapper?.querySelector('code');
    if (!codeEl) return;
    navigator.clipboard.writeText(codeEl.textContent ?? '').then(() => {
      btn.innerHTML = '<span class="ms" style="font-size:14px">check</span>';
      btn.classList.add('copied');
      setTimeout(() => {
        btn.innerHTML = '<span class="ms" style="font-size:14px">content_copy</span>';
        btn.classList.remove('copied');
      }, 1500);
    });
  });
}