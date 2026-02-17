// Pawz — Prompt Injection Scanner — Public API
// Single import point for the feature.

// Atoms (pure functions)
export {
  scanForInjection,
  isLikelyInjection,
  type InjectionScanResult,
  type InjectionMatch,
  type InjectionSeverity,
  type InjectionPattern,
} from './atoms';

// Molecules (composed features)
export {
  evaluateMessage,
  loadInjectionPolicy,
  saveInjectionPolicy,
  severityColor,
  severityLabel,
  DEFAULT_POLICY,
  type InjectionPolicy,
  type InjectionDecision,
} from './molecules';
