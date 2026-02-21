// Nodes — Pure helpers (no DOM, no IPC)

/** HTML-escape a string */
export function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// ── Compat stubs (called from main.ts — kept to avoid breaking imports) ───
export function loadPairingRequests() {
  /* no pairing in engine mode */
}
export function handleNodePairRequested(_payload: unknown) {
  /* noop */
}
export function handleNodePairResolved(_payload: unknown) {
  /* noop */
}
export function configureCallbacks(_opts: Record<string, unknown>) {
  /* noop */
}
