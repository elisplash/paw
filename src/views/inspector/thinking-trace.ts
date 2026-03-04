// Inspector — Thinking Trace Renderer
// Renders the collapsible chain-of-thought reasoning trace
// from extended-thinking models (Claude, Gemini).

import { escHtml } from '../../components/helpers';
import type { InspectorThinkingChunk } from './atoms';

// ── Render ────────────────────────────────────────────────────────────

/** Render the thinking trace section. */
export function renderThinkingTrace(chunks: InspectorThinkingChunk[]): string {
  if (chunks.length === 0) return '';

  // Combine all chunks into a single text
  const fullText = chunks.map((c) => c.text).join('');
  const charCount = fullText.length;

  // Truncate for display (show last N chars if very long)
  const MAX_DISPLAY = 2000;
  const displayText = charCount > MAX_DISPLAY ? `\u2026${fullText.slice(-MAX_DISPLAY)}` : fullText;

  return `
    <div class="inspector-section inspector-thinking">
      <details class="inspector-collapsible">
        <summary class="inspector-section-title">
          <span class="ms ms-xs">psychology</span>
          Thinking (${formatCharCount(charCount)})
        </summary>
        <div class="inspector-thinking-body">
          <pre class="inspector-thinking-pre">${escHtml(displayText)}</pre>
        </div>
      </details>
    </div>
  `;
}

// ── Helpers ───────────────────────────────────────────────────────────

/** Format character count with K suffix for large numbers. */
function formatCharCount(count: number): string {
  if (count < 1000) return `${count} chars`;
  return `${(count / 1000).toFixed(1)}k chars`;
}
