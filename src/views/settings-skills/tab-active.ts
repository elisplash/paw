// My Skills — Active Tab
// Shows all currently enabled/connected items: skills, integrations, MCP servers, extensions.

import type { EngineSkillStatus, McpServerStatus } from '../../engine';
import { renderSkillCard, fromEngineSkill, type SkillCardData } from '../../components/molecules/skill-card';

// ── Types ──────────────────────────────────────────────────────────────

export interface ActiveTabData {
  skills: EngineSkillStatus[];
  mcpStatuses: McpServerStatus[];
}

// ── Render ─────────────────────────────────────────────────────────────

export function renderActiveTab(data: ActiveTabData): string {
  const enabledSkills = data.skills.filter((s) => s.enabled);
  const connectedMcp = data.mcpStatuses.filter((s) => s.connected);

  if (enabledSkills.length === 0 && connectedMcp.length === 0) {
    return `
    <div style="text-align:center;padding:48px 24px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">check_circle</span>
      <h3 style="margin:0 0 8px;font-size:16px;font-weight:600;color:var(--text-primary)">No active skills</h3>
      <p style="color:var(--text-muted);font-size:13px;margin:0 0 16px;max-width:400px;margin-inline:auto">
        Enable skills from the <strong>Integrations</strong> or <strong>Tools</strong> tabs to give your agent superpowers.
      </p>
    </div>`;
  }

  const cards: string[] = [];

  // Active built-in / TOML skills → unified card
  for (const skill of enabledSkills) {
    const cardData = fromEngineSkill(skill);
    // Override action to "none" — Active tab is read-only overview
    cardData.action = { type: 'none' };
    cards.push(renderSkillCard(cardData));
  }

  // Connected MCP servers → unified card
  for (const mcp of connectedMcp) {
    const cardData: SkillCardData = {
      id: mcp.id,
      name: mcp.name ?? mcp.id,
      description: `${mcp.tool_count ?? 0} tool${(mcp.tool_count ?? 0) !== 1 ? 's' : ''} available`,
      icon: 'dns',
      tier: 'mcp',
      status: 'active',
      statusLabel: 'Connected',
      toolCount: mcp.tool_count ?? 0,
      hasWidget: false,
      hasMcp: true,
      verified: false,
      action: { type: 'none' },
      dataAttrs: { 'mcp-id': mcp.id },
    };
    cards.push(renderSkillCard(cardData));
  }

  return `<div class="skills-card-grid">${cards.join('')}</div>`;
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindActiveTabEvents(): void {
  // Active tab is read-only overview — no events needed
  // Users navigate to other tabs to configure
}
