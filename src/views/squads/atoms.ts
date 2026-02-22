// src/views/squads/atoms.ts — Squads view rendering helpers

import type { EngineSquad, EngineSquadMember } from '../../engine/atoms/types';
import { escHtml } from '../../components/helpers';

/** Render a single squad card for the list sidebar. */
export function renderSquadCard(squad: EngineSquad, isActive: boolean): string {
  const memberCount = squad.members.length;
  const statusClass = squad.status === 'active' ? 'active' : 'paused';
  return `<div class="squad-card${isActive ? ' active' : ''}" data-squad-id="${escHtml(squad.id)}">
    <div class="squad-card-header">
      <span class="squad-card-name">${escHtml(squad.name)}</span>
      <span class="squad-card-status ${statusClass}">${escHtml(squad.status)}</span>
    </div>
    <div class="squad-card-goal">${escHtml(squad.goal || 'No goal set')}</div>
    <div class="squad-card-meta">${memberCount} member${memberCount !== 1 ? 's' : ''}</div>
  </div>`;
}

/** Render the squad detail panel. */
export function renderSquadDetail(squad: EngineSquad): string {
  const memberRows = squad.members
    .map(
      (m) => `<div class="squad-member-row" data-agent-id="${escHtml(m.agent_id)}">
      <span class="squad-member-name">${escHtml(m.agent_id)}</span>
      <span class="squad-member-role ${m.role === 'coordinator' ? 'coordinator' : ''}">${escHtml(m.role)}</span>
      <button class="btn btn-ghost btn-sm squad-remove-member" data-agent-id="${escHtml(m.agent_id)}" title="Remove member">×</button>
    </div>`,
    )
    .join('');

  return `<div class="squad-detail-header">
    <h2 class="squad-detail-name">${escHtml(squad.name)}</h2>
    <div class="squad-detail-actions">
      <button class="btn btn-ghost btn-sm" id="squad-edit-btn">Edit</button>
      <button class="btn btn-danger btn-sm" id="squad-delete-btn">Delete</button>
    </div>
  </div>
  <div class="squad-detail-goal">
    <label>Goal</label>
    <p>${escHtml(squad.goal || 'No goal set')}</p>
  </div>
  <div class="squad-detail-members">
    <div class="squad-members-header">
      <h3>Members</h3>
      <button class="btn btn-ghost btn-sm" id="squad-add-member-btn">+ Add Member</button>
    </div>
    <div class="squad-member-list" id="squad-member-list">
      ${memberRows || '<div class="squad-members-empty">No members yet</div>'}
    </div>
  </div>
  <div class="squad-detail-messages">
    <h3>Squad Messages</h3>
    <div class="squad-message-feed" id="squad-message-feed">
      <div class="squad-messages-empty">No messages yet. Squad members can broadcast messages using the squad_broadcast tool.</div>
    </div>
  </div>`;
}

/** Build select options for agents not already in the squad. */
export function buildAgentOptions(
  agents: Array<{ id: string; name: string }>,
  existingMembers: EngineSquadMember[],
): string {
  const memberIds = new Set(existingMembers.map((m) => m.agent_id));
  return agents
    .filter((a) => !memberIds.has(a.id))
    .map((a) => `<option value="${escHtml(a.id)}">${escHtml(a.name)}</option>`)
    .join('');
}
