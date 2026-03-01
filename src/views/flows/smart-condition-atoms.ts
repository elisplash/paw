// ─────────────────────────────────────────────────────────────────────────────
// Smart Condition Evaluation — Atoms (Pure Logic)
// Structured expression parser and evaluator for flow condition nodes.
// Supports JSONPath-like data access, comparisons, and boolean logic.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

/** Result of evaluating a smart condition. */
export interface ConditionEvalResult {
  /** Boolean result of the condition */
  result: boolean;
  /** How the condition was evaluated */
  method: 'structured' | 'fuzzy';
  /** Human-readable explanation of the evaluation */
  explanation: string;
}

/** A parsed condition expression. */
interface ParsedCondition {
  left: string;
  operator: string;
  right: string;
}

// ── Expression Parser ──────────────────────────────────────────────────────

/** Supported comparison operators. */
const COMPARISON_OPS = ['===', '!==', '>=', '<=', '==', '!=', '>', '<'] as const;

/** Supported logical connectors. */
const LOGICAL_OPS = ['&&', '||'] as const;

/**
 * Parse a simple comparison expression.
 * Supports: `left op right` where op is a comparison operator.
 */
export function parseConditionExpr(expr: string): ParsedCondition | null {
  const trimmed = expr.trim();

  // Try each operator (longest first to avoid partial matches)
  for (const op of COMPARISON_OPS) {
    const idx = trimmed.indexOf(op);
    if (idx > 0 && idx < trimmed.length - op.length) {
      return {
        left: trimmed.slice(0, idx).trim(),
        operator: op,
        right: trimmed.slice(idx + op.length).trim(),
      };
    }
  }

  return null;
}

// ── Data Access (JSONPath-like) ────────────────────────────────────────────

/**
 * Resolve a dot-path expression against parsed data.
 * Supports: `input`, `data.field`, `data.nested.field`, `data.arr[0]`, `data.length`
 */
export function resolvePath(path: string, data: unknown, rawInput?: string): unknown {
  const trimmed = path.trim();

  // Special keywords
  if (trimmed === 'input') return rawInput ?? '';
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;
  if (trimmed === 'null') return null;
  if (trimmed === 'undefined') return undefined;

  // Numeric literal
  const num = Number(trimmed);
  if (!isNaN(num) && trimmed !== '') return num;

  // String literal (quoted)
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }

  // Dot-path traversal
  const parts = trimmed.split(/\.|\[(\d+)\]/).filter(Boolean);
  let current: unknown = data;

  // If path starts with 'input', use rawInput as root
  if (parts[0] === 'input') {
    if (parts.length === 1) return rawInput ?? '';
    // Try to parse rawInput as JSON for deeper access
    try {
      current = JSON.parse(rawInput ?? '');
    } catch {
      return rawInput ?? '';
    }
    parts.shift(); // Remove 'input' prefix
  } else if (
    parts[0] === 'data' &&
    data != null &&
    typeof data === 'object' &&
    !('data' in (data as Record<string, unknown>))
  ) {
    current = data;
    parts.shift(); // Remove 'data' prefix — only when data param IS the data root
  }

  for (const part of parts) {
    if (current == null) return undefined;
    if (typeof current !== 'object') return undefined;

    const idx = Number(part);
    if (!isNaN(idx) && Array.isArray(current)) {
      current = current[idx];
    } else {
      current = (current as Record<string, unknown>)[part];
    }
  }

  return current;
}

// ── Comparators ────────────────────────────────────────────────────────────

/**
 * Compare two values using the given operator.
 */
export function compareValues(left: unknown, op: string, right: unknown): boolean {
  // Coerce to comparable types
  const l = typeof left === 'string' ? left : left;
  const r = typeof right === 'string' ? right : right;

  switch (op) {
    case '===':
      return l === r;
    case '!==':
      return l !== r;
    case '==':
      // eslint-disable-next-line eqeqeq
      return l == r;
    case '!=':
      // eslint-disable-next-line eqeqeq
      return l != r;
    case '>':
      return Number(l) > Number(r);
    case '<':
      return Number(l) < Number(r);
    case '>=':
      return Number(l) >= Number(r);
    case '<=':
      return Number(l) <= Number(r);
    default:
      return false;
  }
}

// ── Built-in Checks ────────────────────────────────────────────────────────

/**
 * Check for built-in condition shortcuts (no parsing needed).
 */
export function evaluateBuiltinCondition(expr: string): ConditionEvalResult | null {
  const normalized = expr.trim().toLowerCase();

  // Boolean literals
  if (normalized === 'true' || normalized === 'yes') {
    return { result: true, method: 'structured', explanation: 'Literal true' };
  }
  if (normalized === 'false' || normalized === 'no') {
    return { result: false, method: 'structured', explanation: 'Literal false' };
  }

  // Existence / truthiness checks
  if (normalized === 'input' || normalized === 'has_input') {
    return null; // Needs data context — handled in evaluateSmartCondition
  }

  return null;
}

// ── Main Evaluator ─────────────────────────────────────────────────────────

/**
 * Evaluate a condition expression structurally against input data.
 * Returns null if the expression requires AI (fuzzy/semantic) evaluation.
 *
 * Supports:
 * - Boolean literals: `true`, `false`, `yes`, `no`
 * - Comparisons: `data.status === 200`, `input.length > 0`, `data.count >= 10`
 * - Property checks: `data.items`, `input` (truthy check)
 * - String comparisons: `data.type === "error"`, `input == "ready"`
 * - Compound: `data.status === 200 && data.items.length > 0` (simple && / ||)
 */
export function evaluateSmartCondition(expr: string, rawInput: string): ConditionEvalResult | null {
  const trimmed = expr.trim();
  if (!trimmed) return null;

  // Check built-in shortcuts
  const builtin = evaluateBuiltinCondition(trimmed);
  if (builtin) return builtin;

  // Parse input data
  let parsedData: unknown = null;
  try {
    parsedData = JSON.parse(rawInput);
  } catch {
    parsedData = rawInput;
  }

  // Handle compound expressions (simple && / ||)
  for (const logicOp of LOGICAL_OPS) {
    if (trimmed.includes(logicOp)) {
      const parts = trimmed.split(logicOp).map((p) => p.trim());
      const results = parts.map((p) => evaluateSmartCondition(p, rawInput));

      // If any sub-expression needs AI, the whole thing does
      if (results.some((r) => r === null)) return null;

      const boolResults = results.map((r) => r!.result);
      const combined = logicOp === '&&' ? boolResults.every(Boolean) : boolResults.some(Boolean);

      return {
        result: combined,
        method: 'structured',
        explanation: `${parts.join(` ${logicOp} `)} → ${combined}`,
      };
    }
  }

  // Truthiness check (single property path, no operator)
  const parsed = parseConditionExpr(trimmed);
  if (!parsed) {
    // Try as a truthiness check on a data path
    const value = resolvePath(trimmed, parsedData, rawInput);
    if (value !== undefined) {
      const result = !!value && value !== '' && value !== 0 && value !== 'false';
      return {
        result,
        method: 'structured',
        explanation: `${trimmed} = ${JSON.stringify(value)} → ${result}`,
      };
    }
    // Can't evaluate structurally — needs AI
    return null;
  }

  // Resolve left and right values
  const leftVal = resolvePath(parsed.left, parsedData, rawInput);
  const rightVal = resolvePath(parsed.right, parsedData, rawInput);

  // If we can't resolve either side, need AI
  if (leftVal === undefined && rightVal === undefined) return null;

  const result = compareValues(leftVal, parsed.operator, rightVal);
  return {
    result,
    method: 'structured',
    explanation: `${parsed.left} (${JSON.stringify(leftVal)}) ${parsed.operator} ${parsed.right} (${JSON.stringify(rightVal)}) → ${result}`,
  };
}
