// ─────────────────────────────────────────────────────────────────────────────
// Flow Self-Healing — Atoms (Pure Logic)
// Diagnosis prompt building, fix proposal types, error analysis.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import type { NodeExecConfig } from './executor-atoms';

// ── Types ──────────────────────────────────────────────────────────────────

/** Diagnosis request for a failed node. */
export interface DiagnosisRequest {
  /** The failed node */
  node: FlowNode;
  /** Node execution config */
  config: NodeExecConfig;
  /** The error that occurred */
  error: string;
  /** Input data that was fed to the node */
  input: string;
  /** Upstream node outputs (for context) */
  upstreamOutputs?: Array<{ nodeId: string; label: string; output: string }>;
  /** The full graph for structural context */
  graph: FlowGraph;
}

/** A proposed fix from the self-healing agent. */
export interface FixProposal {
  /** Human-readable diagnosis */
  diagnosis: string;
  /** What the fix changes */
  description: string;
  /** Config fields to update (partial patch) */
  configPatch?: Record<string, unknown>;
  /** New label (if rename needed) */
  newLabel?: string;
  /** Confidence level 0–1 */
  confidence: number;
  /** Whether the fix can be auto-applied (vs needs user approval) */
  autoApplicable: boolean;
}

/** Full diagnosis result from the healing agent. */
export interface DiagnosisResult {
  /** Root cause analysis */
  rootCause: string;
  /** Detailed explanation */
  explanation: string;
  /** Ordered fix proposals (most confident first) */
  fixes: FixProposal[];
  /** Whether retry with the same config might work (transient error) */
  isTransient: boolean;
}

// ── Error Classification ───────────────────────────────────────────────────

/** Known error patterns and their categories. */
export type ErrorCategory =
  | 'timeout' // Request timed out
  | 'rate-limit' // API rate limit hit
  | 'auth' // Authentication/authorization failure
  | 'network' // Network connectivity issue
  | 'invalid-input' // Bad input data
  | 'config' // Missing or invalid node configuration
  | 'code-error' // Code node runtime error
  | 'api-error' // External API returned error
  | 'unknown'; // Unrecognized error

const ERROR_PATTERNS: Array<{ pattern: RegExp; category: ErrorCategory }> = [
  { pattern: /timeout|timed?\s*out|ETIMEDOUT/i, category: 'timeout' },
  { pattern: /rate\s*limit|429|too many requests/i, category: 'rate-limit' },
  { pattern: /unauthorized|forbidden|401|403|auth/i, category: 'auth' },
  { pattern: /ECONNREFUSED|ENOTFOUND|network|DNS/i, category: 'network' },
  {
    pattern: /invalid.*input|parse.*error|JSON\.parse|unexpected token/i,
    category: 'invalid-input',
  },
  { pattern: /no.*configured|missing.*config|undefined.*config/i, category: 'config' },
  { pattern: /Code error:|Blocked:|sandbox/i, category: 'code-error' },
  { pattern: /4\d{2}|5\d{2}|API.*error|server error/i, category: 'api-error' },
];

/**
 * Classify an error message into a known category.
 */
export function classifyError(error: string): ErrorCategory {
  for (const { pattern, category } of ERROR_PATTERNS) {
    if (pattern.test(error)) return category;
  }
  return 'unknown';
}

/**
 * Determine if an error is likely transient (retry might succeed).
 */
export function isTransientError(error: string): boolean {
  const category = classifyError(error);
  return category === 'timeout' || category === 'rate-limit' || category === 'network';
}

/**
 * Suggest quick fixes based on error category (no AI needed).
 */
export function suggestQuickFixes(
  error: string,
  node: FlowNode,
  config: NodeExecConfig,
): FixProposal[] {
  const category = classifyError(error);
  const fixes: FixProposal[] = [];

  switch (category) {
    case 'timeout':
      fixes.push({
        diagnosis: 'Request timed out.',
        description: `Increase timeout from ${(config.timeoutMs ?? 120000) / 1000}s to ${((config.timeoutMs ?? 120000) * 2) / 1000}s`,
        configPatch: { timeoutMs: (config.timeoutMs ?? 120000) * 2 },
        confidence: 0.7,
        autoApplicable: true,
      });
      if ((config.maxRetries ?? 0) < 2) {
        fixes.push({
          diagnosis: 'Request timed out.',
          description: 'Add retry with backoff (2 retries, 2s delay)',
          configPatch: { maxRetries: 2, retryDelayMs: 2000 },
          confidence: 0.6,
          autoApplicable: true,
        });
      }
      break;

    case 'rate-limit':
      fixes.push({
        diagnosis: 'API rate limit exceeded.',
        description: 'Add retry with longer backoff (3 retries, 5s delay, 3x backoff)',
        configPatch: { maxRetries: 3, retryDelayMs: 5000, retryBackoff: 3 },
        confidence: 0.8,
        autoApplicable: true,
      });
      break;

    case 'auth':
      fixes.push({
        diagnosis: 'Authentication failed.',
        description:
          'Check credential configuration — the API key or token may be expired or missing.',
        confidence: 0.5,
        autoApplicable: false,
      });
      break;

    case 'config':
      if (!config.prompt && ['agent', 'tool'].includes(node.kind)) {
        fixes.push({
          diagnosis: 'Node has no prompt configured.',
          description: 'Add a prompt describing what this step should do.',
          configPatch: { prompt: `Execute the task: ${node.label}` },
          confidence: 0.8,
          autoApplicable: true,
        });
      }
      break;

    case 'code-error':
      fixes.push({
        diagnosis: 'Code execution error.',
        description:
          'Review the JavaScript code for syntax or runtime errors. Check that the code avoids forbidden patterns (window, document, fetch, eval).',
        confidence: 0.4,
        autoApplicable: false,
      });
      break;

    case 'invalid-input':
      fixes.push({
        diagnosis: 'Input data format is invalid.',
        description: 'Add a data transform node before this step to clean/parse the input.',
        confidence: 0.5,
        autoApplicable: false,
      });
      break;

    default:
      break;
  }

  return fixes;
}

// ── Diagnosis Prompt Builder ───────────────────────────────────────────────

/**
 * Build the system prompt for the self-healing diagnosis agent.
 */
export function buildDiagnosisSystemPrompt(): string {
  return `You are a flow debugging assistant for OpenPawz. When a flow node fails, you analyze the error, upstream data, and node configuration to diagnose the root cause and propose fixes.

## Output Format
Respond with ONLY a JSON object:
{
  "rootCause": "Brief root cause description",
  "explanation": "Detailed analysis of what went wrong and why",
  "fixes": [
    {
      "diagnosis": "What this fix addresses",
      "description": "What the fix changes",
      "configPatch": { "key": "value" },
      "confidence": 0.8,
      "autoApplicable": true
    }
  ],
  "isTransient": false
}

## Rules
1. configPatch should only contain valid node config keys
2. Order fixes by confidence (highest first)
3. Set autoApplicable=true only for safe, reversible changes
4. isTransient=true for timeouts, rate limits, network issues
5. Be specific about what went wrong — vague diagnoses are unhelpful`;
}

/**
 * Build the user prompt for diagnosing a failed node.
 */
export function buildDiagnosisPrompt(request: DiagnosisRequest): string {
  const parts: string[] = [];

  parts.push(`## Failed Node`);
  parts.push(`- Kind: ${request.node.kind}`);
  parts.push(`- Label: ${request.node.label}`);
  parts.push(`- Description: ${request.node.description || '(none)'}`);
  parts.push(`\n## Error\n\`\`\`\n${request.error}\n\`\`\``);
  parts.push(`\n## Input Data\n\`\`\`\n${request.input || '(empty)'}\n\`\`\``);

  // Include relevant config
  const configStr = Object.entries(request.config)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `  ${k}: ${JSON.stringify(v)}`)
    .join('\n');
  if (configStr) {
    parts.push(`\n## Node Config\n\`\`\`\n${configStr}\n\`\`\``);
  }

  // Upstream context
  if (request.upstreamOutputs?.length) {
    parts.push('\n## Upstream Outputs');
    for (const up of request.upstreamOutputs) {
      const preview = up.output.length > 300 ? `${up.output.slice(0, 300)}…` : up.output;
      parts.push(`- **${up.label}**: ${preview}`);
    }
  }

  // Graph structure context
  parts.push(`\n## Flow: "${request.graph.name}" (${request.graph.nodes.length} nodes)`);

  return parts.join('\n');
}

/**
 * Parse the AI diagnosis response into a DiagnosisResult.
 */
export function parseDiagnosisResponse(response: string): DiagnosisResult | null {
  let jsonStr = response.trim();
  const codeBlockMatch = jsonStr.match(/```(?:json)?\s*\n?([\s\S]*?)\n?```/);
  if (codeBlockMatch) {
    jsonStr = codeBlockMatch[1].trim();
  }

  try {
    const parsed = JSON.parse(jsonStr);
    if (!parsed.rootCause || !Array.isArray(parsed.fixes)) return null;

    return {
      rootCause: parsed.rootCause,
      explanation: parsed.explanation || parsed.rootCause,
      fixes: (parsed.fixes as FixProposal[]).map((f) => ({
        diagnosis: f.diagnosis || '',
        description: f.description || '',
        configPatch: f.configPatch,
        newLabel: f.newLabel,
        confidence: typeof f.confidence === 'number' ? f.confidence : 0.5,
        autoApplicable: f.autoApplicable ?? false,
      })),
      isTransient: parsed.isTransient ?? false,
    };
  } catch {
    return null;
  }
}

/**
 * Apply a fix proposal to a node's config. Returns the patched config.
 */
export function applyFixToConfig(
  currentConfig: Record<string, unknown>,
  fix: FixProposal,
): Record<string, unknown> {
  if (!fix.configPatch) return currentConfig;
  return { ...currentConfig, ...fix.configPatch };
}
