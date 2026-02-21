// Settings: Tailscale — Pure helpers (no DOM, no IPC)

/** HTML-escape a string */
export function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/** Create a styled button element */
export function makeBtn(label: string, cls: string, handler: () => void): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.className = `btn ${cls} btn-sm`;
  btn.textContent = label;
  btn.addEventListener('click', handler);
  return btn;
}
