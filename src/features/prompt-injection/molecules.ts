// Pawz — Prompt Injection Scanner — Molecules
// Composed atoms: scan + log + UI feedback for injection detection.

import { scanForInjection, type InjectionScanResult, type InjectionSeverity } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────

export interface InjectionPolicy {
  enabled: boolean;
  blockCritical: boolean; // Auto-block critical severity
  blockHigh: boolean; // Auto-block high severity
  warnMedium: boolean; // Show warning for medium severity
  logAll: boolean; // Log all scan results (even clean)
  scoreThreshold: number; // Score above this = blocked (0–100)
  channelScanEnabled: boolean; // Scan incoming channel messages
}

export interface InjectionDecision {
  action: 'allow' | 'warn' | 'block';
  scan: InjectionScanResult;
  reason: string;
}

// ── Default policy ─────────────────────────────────────────────────────

const POLICY_KEY = 'paw_injection_policy';

export const DEFAULT_POLICY: InjectionPolicy = {
  enabled: true,
  blockCritical: true,
  blockHigh: false,
  warnMedium: true,
  logAll: false,
  scoreThreshold: 40,
  channelScanEnabled: true,
};

// ── Policy persistence (atom helpers) ──────────────────────────────────

export function loadInjectionPolicy(): InjectionPolicy {
  try {
    const raw = localStorage.getItem(POLICY_KEY);
    if (raw) return { ...DEFAULT_POLICY, ...JSON.parse(raw) };
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_POLICY };
}

export function saveInjectionPolicy(policy: InjectionPolicy): void {
  localStorage.setItem(POLICY_KEY, JSON.stringify(policy));
}

// ── Decision engine (molecule) ─────────────────────────────────────────

/**
 * Evaluate a message against injection policy.
 * Returns a decision: allow, warn, or block.
 */
export function evaluateMessage(text: string, policy?: InjectionPolicy): InjectionDecision {
  const p = policy ?? loadInjectionPolicy();
  const scan = scanForInjection(text);

  // Policy disabled — always allow
  if (!p.enabled) {
    return { action: 'allow', scan, reason: 'Injection scanning disabled' };
  }

  // No injection detected
  if (!scan.isInjection) {
    return { action: 'allow', scan, reason: 'Clean — no injection patterns detected' };
  }

  // Score threshold check
  if (scan.score >= p.scoreThreshold) {
    return {
      action: 'block',
      scan,
      reason: `Injection score ${scan.score} exceeds threshold ${p.scoreThreshold}`,
    };
  }

  // Severity-based checks
  if (p.blockCritical && scan.severity === 'critical') {
    return {
      action: 'block',
      scan,
      reason: `Critical injection detected: ${scan.matches[0]?.description}`,
    };
  }

  if (p.blockHigh && scan.severity === 'high') {
    return {
      action: 'block',
      scan,
      reason: `High-severity injection detected: ${scan.matches[0]?.description}`,
    };
  }

  // Medium severity — warn
  if (p.warnMedium && (scan.severity === 'medium' || scan.severity === 'high')) {
    return {
      action: 'warn',
      scan,
      reason: `Possible injection: ${scan.matches[0]?.description}`,
    };
  }

  // Low severity — allow with info
  return {
    action: 'allow',
    scan,
    reason: `Low-severity pattern detected: ${scan.matches[0]?.description}`,
  };
}

// ── Severity formatting helpers ────────────────────────────────────────

export function severityColor(severity: InjectionSeverity): string {
  switch (severity) {
    case 'critical':
      return '#e74c3c';
    case 'high':
      return '#e67e22';
    case 'medium':
      return '#f39c12';
    case 'low':
      return '#95a5a6';
  }
}

export function severityLabel(severity: InjectionSeverity): string {
  switch (severity) {
    case 'critical':
      return 'CRITICAL';
    case 'high':
      return 'HIGH';
    case 'medium':
      return 'MEDIUM';
    case 'low':
      return 'LOW';
  }
}
