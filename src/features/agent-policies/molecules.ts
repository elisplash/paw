// ─────────────────────────────────────────────────────────────────────────────
// Agent Tool Policies — Molecules
// Composed behaviours: load/save policies, enforce during agent execution.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type ToolPolicy,
  type PolicyDecision,
  DEFAULT_POLICY,
  checkToolPolicy,
  filterToolsByPolicy,
} from './atoms';

// ── Storage ────────────────────────────────────────────────────────────────

const STORAGE_KEY = 'paw_agent_tool_policies';

/**
 * Load all agent tool policies from localStorage.
 * Returns a map of agentId → ToolPolicy.
 */
export function loadAllPolicies(): Record<string, ToolPolicy> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    return JSON.parse(raw);
  } catch {
    return {};
  }
}

/**
 * Get the tool policy for a specific agent.
 * Returns DEFAULT_POLICY if none is set.
 */
export function getAgentPolicy(agentId: string): ToolPolicy {
  const all = loadAllPolicies();
  return all[agentId] ?? { ...DEFAULT_POLICY };
}

/**
 * Save a tool policy for a specific agent.
 */
export function setAgentPolicy(agentId: string, policy: ToolPolicy): void {
  const all = loadAllPolicies();
  all[agentId] = policy;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(all));
}

/**
 * Remove a tool policy for an agent (reverts to default).
 */
export function removeAgentPolicy(agentId: string): void {
  const all = loadAllPolicies();
  delete all[agentId];
  localStorage.setItem(STORAGE_KEY, JSON.stringify(all));
}

// ── Enforcement ────────────────────────────────────────────────────────────

/**
 * Evaluate whether a tool call should be allowed for a given agent.
 * This is the main enforcement function called during agent execution.
 */
export function enforceToolPolicy(agentId: string, toolName: string): PolicyDecision {
  const policy = getAgentPolicy(agentId);
  return checkToolPolicy(toolName, policy);
}

/**
 * Get the list of tool names an agent is allowed to use.
 * Use this to filter tool definitions sent to the AI model.
 */
export function getAgentAllowedTools(agentId: string, allToolNames: string[]): string[] {
  const policy = getAgentPolicy(agentId);
  return filterToolsByPolicy(allToolNames, policy);
}

/**
 * Build a policy summary string for display in the agent card.
 */
export function getAgentPolicySummary(agentId: string): string {
  const policy = getAgentPolicy(agentId);
  switch (policy.mode) {
    case 'unrestricted':
      return 'Unrestricted';
    case 'allowlist':
      return `${policy.allowed.length} tools allowed`;
    case 'denylist':
      return `${policy.denied.length} tools blocked`;
    default:
      return 'Unrestricted';
  }
}
