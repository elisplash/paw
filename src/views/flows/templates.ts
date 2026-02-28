// ─────────────────────────────────────────────────────────────────────────────
// Flow Templates — Aggregator Hub
// Combines template collections from sub-modules into a single export.
//
// Sub-modules:
//   templates-core.ts      — AI & Agents, Communication, DevOps
//   templates-workflows.ts — Productivity, Data, Research, Social
//   templates-advanced.ts  — Finance, Support, AI Patterns, AI Superpowers
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowTemplate } from './atoms';
import { TEMPLATES_CORE } from './templates-core';
import { TEMPLATES_WORKFLOWS } from './templates-workflows';
import { TEMPLATES_ADVANCED } from './templates-advanced';

export const FLOW_TEMPLATES: FlowTemplate[] = [
  ...TEMPLATES_CORE,
  ...TEMPLATES_WORKFLOWS,
  ...TEMPLATES_ADVANCED,
];
