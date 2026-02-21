// Pawz — Prompt Injection Scanner
// Atoms: Pure detection functions for identifying prompt injection attempts
// in incoming messages from channels and chat.

// ── Types ──────────────────────────────────────────────────────────────

export type InjectionSeverity = 'critical' | 'high' | 'medium' | 'low';

export interface InjectionPattern {
  pattern: RegExp;
  severity: InjectionSeverity;
  category: string;
  description: string;
}

export interface InjectionScanResult {
  isInjection: boolean;
  severity: InjectionSeverity | null;
  matches: InjectionMatch[];
  score: number; // 0–100 composite risk score
  sanitizedText: string; // text with injection markers stripped
}

export interface InjectionMatch {
  pattern: string;
  severity: InjectionSeverity;
  category: string;
  description: string;
  matchedText: string;
  position: number;
}

// ── Injection patterns ─────────────────────────────────────────────────

const INJECTION_PATTERNS: InjectionPattern[] = [
  // ── CRITICAL: Direct system prompt override ──
  {
    pattern:
      /\bignore\s+(all\s+)?(previous|prior|above|earlier)\s+(instructions?|prompts?|rules?|directives?|context)\b/i,
    severity: 'critical',
    category: 'override',
    description: 'Attempts to override system prompt',
  },
  {
    pattern:
      /\bdisregard\s+(all\s+)?(previous|prior|above|earlier)\s+(instructions?|prompts?|rules?)\b/i,
    severity: 'critical',
    category: 'override',
    description: 'Disregard previous instructions',
  },
  {
    pattern:
      /\bforget\s+(all\s+)?(previous|prior|your|earlier)\s+(instructions?|prompts?|rules?|training|context)\b/i,
    severity: 'critical',
    category: 'override',
    description: 'Forget previous instructions',
  },
  {
    pattern: /\byou\s+are\s+now\s+(a|an|the)\s+/i,
    severity: 'critical',
    category: 'identity',
    description: 'Attempts to redefine agent identity',
  },
  {
    pattern: /\bnew\s+instructions?:\s/i,
    severity: 'critical',
    category: 'override',
    description: 'Injects new instructions',
  },
  {
    pattern: /\bsystem\s*:\s/i,
    severity: 'critical',
    category: 'override',
    description: 'Fake system message injection',
  },
  {
    pattern: /\b(SYSTEM|ADMIN|ROOT)\s+(OVERRIDE|COMMAND|DIRECTIVE)[\s:]/i,
    severity: 'critical',
    category: 'override',
    description: 'Fake system/admin override command',
  },

  // ── CRITICAL: Role confusion / jailbreak ──
  {
    pattern: /\bact\s+as\s+(if\s+)?(you\s+)?(are|were)\s+(a\s+)?different/i,
    severity: 'critical',
    category: 'jailbreak',
    description: 'Role confusion — act as different entity',
  },
  {
    pattern: /\bDAN\s+(mode|prompt|jailbreak)\b/i,
    severity: 'critical',
    category: 'jailbreak',
    description: 'Known DAN jailbreak pattern',
  },
  {
    pattern: /\bdo\s+anything\s+now\b/i,
    severity: 'critical',
    category: 'jailbreak',
    description: 'DAN (Do Anything Now) jailbreak',
  },
  {
    pattern: /\bjailbreak(ed|ing)?\b/i,
    severity: 'high',
    category: 'jailbreak',
    description: 'Explicit jailbreak mention',
  },
  {
    pattern: /\bdeveloper\s+mode\s+(enabled|on|activated)\b/i,
    severity: 'critical',
    category: 'jailbreak',
    description: 'Fake developer mode activation',
  },

  // ── HIGH: Prompt leaking ──
  {
    pattern:
      /\b(show|reveal|tell|display|print|output|repeat|echo)\s+(me\s+)?(your|the)\s+(system\s*)?(prompt|instructions?|rules?|configuration|config|context)\b/i,
    severity: 'high',
    category: 'leaking',
    description: 'Attempts to extract system prompt',
  },
  {
    pattern:
      /\bwhat\s+(are|is)\s+your\s+(system\s*)?(prompt|instructions?|rules?|directives?|guidelines?)\b/i,
    severity: 'high',
    category: 'leaking',
    description: 'Asks for system prompt content',
  },
  {
    pattern: /\brepeat\s+(the\s+)?(text|words?|content)\s+above\b/i,
    severity: 'high',
    category: 'leaking',
    description: 'Repeat text above (prompt leak)',
  },

  // ── HIGH: Encoded/obfuscated injection ──
  {
    pattern: /\b(base64|rot13|hex)\s*(decode|encode|convert)\b/i,
    severity: 'high',
    category: 'obfuscation',
    description: 'Encoding/decoding to bypass filters',
  },
  {
    pattern: /&#x[0-9a-f]{2,4};/i,
    severity: 'high',
    category: 'obfuscation',
    description: 'HTML entity encoding detected',
  },

  // ── HIGH: Tool/exec injection ──
  {
    pattern: /\b(run|execute|call|invoke)\s+(the\s+)?(tool|function|command)\s*[:`'"]/i,
    severity: 'high',
    category: 'tool_injection',
    description: 'Attempts to force tool execution',
  },
  {
    pattern: /\btool_call\s*\(/i,
    severity: 'high',
    category: 'tool_injection',
    description: 'Direct tool_call injection',
  },

  // ── MEDIUM: Social engineering ──
  {
    pattern:
      /\b(pretend|imagine|suppose|assume)\s+(that\s+)?(you\s+)?(are|have|can|don't|do not)\b/i,
    severity: 'medium',
    category: 'social',
    description: 'Social engineering — pretend/imagine scenarios',
  },
  {
    pattern:
      /\b(in\s+)?a\s+(hypothetical|fictional|imaginary|roleplay)\s+(scenario|situation|world|context)\b/i,
    severity: 'medium',
    category: 'social',
    description: 'Hypothetical framing to bypass safety',
  },
  {
    pattern: /\bfor\s+(educational|research|testing|academic)\s+purposes?\s+only\b/i,
    severity: 'medium',
    category: 'social',
    description: 'Educational purposes exemption framing',
  },
  {
    pattern:
      /\bwithout\s+(any\s+)?(restrictions?|limitations?|safet(y|ies)|guardrails?|filters?|censorship)\b/i,
    severity: 'medium',
    category: 'social',
    description: 'Requesting removal of safety restrictions',
  },

  // ── MEDIUM: Hidden text / markup abuse ──
  {
    pattern: /\[INST\]/i,
    severity: 'medium',
    category: 'markup',
    description: 'Llama-style instruction markers',
  },
  {
    pattern: /<\|im_(start|end)\|>/i,
    severity: 'medium',
    category: 'markup',
    description: 'ChatML-style markers',
  },
  {
    pattern: /\bHuman:\s|Assistant:\s|System:\s/,
    severity: 'medium',
    category: 'markup',
    description: 'Role prefix injection (Human:/Assistant:/System:)',
  },
  {
    pattern: /<\/?system>|<\/?user>|<\/?assistant>/i,
    severity: 'medium',
    category: 'markup',
    description: 'XML role tag injection',
  },

  // ── LOW: Suspicious patterns (possible false positives) ──
  {
    pattern:
      /\b(bypass|circumvent|evade|override|disable)\s+(the\s+)?(safety|security|content|moderation|filter)/i,
    severity: 'low',
    category: 'bypass',
    description: 'Bypass safety mention',
  },
];

// ── Weight table for scoring ───────────────────────────────────────────

const SEVERITY_WEIGHTS: Record<InjectionSeverity, number> = {
  critical: 40,
  high: 25,
  medium: 12,
  low: 5,
};

// ── Core scan function (atom) ──────────────────────────────────────────

/**
 * Scan text for prompt injection patterns.
 * Returns a detailed result with all matches, severity, and score.
 */
export function scanForInjection(text: string): InjectionScanResult {
  const matches: InjectionMatch[] = [];
  let maxSeverity: InjectionSeverity | null = null;
  let score = 0;

  const severityOrder: InjectionSeverity[] = ['low', 'medium', 'high', 'critical'];

  for (const ip of INJECTION_PATTERNS) {
    const m = ip.pattern.exec(text);
    if (m) {
      matches.push({
        pattern: ip.pattern.source,
        severity: ip.severity,
        category: ip.category,
        description: ip.description,
        matchedText: m[0],
        position: m.index,
      });

      score += SEVERITY_WEIGHTS[ip.severity];

      if (!maxSeverity || severityOrder.indexOf(ip.severity) > severityOrder.indexOf(maxSeverity)) {
        maxSeverity = ip.severity;
      }
    }
  }

  // Cap at 100
  if (score > 100) score = 100;

  return {
    isInjection: matches.length > 0,
    severity: maxSeverity,
    matches,
    score,
    sanitizedText: sanitizeInjectionMarkers(text),
  };
}

/**
 * Quick boolean check — is this message likely an injection attempt?
 * Uses a threshold score of 20 (one critical or two mediums).
 */
export function isLikelyInjection(text: string, threshold = 20): boolean {
  return scanForInjection(text).score >= threshold;
}

// ── Sanitizer (atom) ───────────────────────────────────────────────────

/**
 * Strip known injection markers from text without removing content.
 * This is a best-effort cleanup — it removes role tags, ChatML markers, etc.
 */
function sanitizeInjectionMarkers(text: string): string {
  let cleaned = text;
  // Remove ChatML markers
  cleaned = cleaned.replace(/<\|im_(start|end)\|>\s*/gi, '');
  // Remove role tags
  cleaned = cleaned.replace(/<\/?(system|user|assistant|human)>/gi, '');
  // Remove Llama instruction markers
  cleaned = cleaned.replace(/\[INST\]|\[\/INST\]/gi, '');
  // Remove fake role prefixes at line starts
  cleaned = cleaned.replace(/^(Human|Assistant|System|User):\s*/gm, '');
  return cleaned.trim();
}
