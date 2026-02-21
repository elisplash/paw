// Settings View — Atoms (pure logic, zero DOM / zero IPC)

// ── Types ──────────────────────────────────────────────────────────────────

export interface ToolRule {
  name: string;
  state: 'allow' | 'ask' | 'deny';
}

// ── Budget helpers ─────────────────────────────────────────────────────────

export const BUDGET_KEY = 'paw-budget-limit';

export function getBudgetLimit(): number | null {
  const saved = localStorage.getItem(BUDGET_KEY);
  if (!saved) return null;
  const n = parseFloat(saved);
  return isNaN(n) || n <= 0 ? null : n;
}

export function setBudgetLimit(limit: number | null) {
  if (limit == null || limit <= 0) {
    localStorage.removeItem(BUDGET_KEY);
  } else {
    localStorage.setItem(BUDGET_KEY, String(limit));
  }
}

// ── Generic file download ──────────────────────────────────────────────────

export function downloadFile(filename: string, content: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
