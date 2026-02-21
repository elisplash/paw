// Research View — Atoms (pure logic, zero DOM / zero IPC)

// ── Types ──────────────────────────────────────────────────────────────────

export type ResearchMode = 'quick' | 'deep';

export interface ProgressPattern {
  regex: RegExp;
  step: string;
}

export const PROGRESS_PATTERNS: ProgressPattern[] = [
  { regex: /searching|search for/i, step: 'Searching the web...' },
  { regex: /reading|fetching|loading/i, step: 'Reading sources...' },
  { regex: /analyzing|analysis/i, step: 'Analyzing content...' },
  { regex: /found|discovered/i, step: 'Found relevant information' },
  { regex: /summarizing|summary/i, step: 'Summarizing findings...' },
];

// ── Pure helpers ───────────────────────────────────────────────────────────

export function extractDomain(url: string): string {
  try {
    return new URL(url).hostname.replace('www.', '');
  } catch {
    return url.slice(0, 30);
  }
}

/**
 * Match streaming text against progress patterns, returning a new step label
 * if one matches (and hasn't already been seen).
 */
export function parseProgressStep(text: string, existingSteps: string[]): string | null {
  for (const { regex, step } of PROGRESS_PATTERNS) {
    if (regex.test(text) && !existingSteps.includes(step)) {
      return step;
    }
  }
  return null;
}

/**
 * Build the prompt sent to the agent based on the query + mode.
 */
export function buildResearchPrompt(query: string, mode: ResearchMode): string {
  return mode === 'deep'
    ? `Research this topic thoroughly and comprehensively. Browse the web, find at least 10 diverse sources, cross-reference information, and provide detailed findings with specific data points, examples, and source URLs. Be exhaustive.\n\nTopic: ${query}`
    : `Research this topic efficiently. Find 3-5 reliable sources, extract key information, and provide a focused summary with the most important findings and source URLs.\n\nTopic: ${query}`;
}

/**
 * Return the timeout (ms) for a given research mode.
 */
export function modeTimeout(mode: ResearchMode): number {
  return mode === 'deep' ? 300_000 : 120_000;
}
