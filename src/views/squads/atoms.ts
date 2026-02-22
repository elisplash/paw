// src/views/squads/atoms.ts — Squads view rendering helpers

import type { EngineSquad, EngineSquadMember, EngineAgentMessage } from '../../engine/atoms/types';
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
  <div class="squad-swarm-status" id="squad-swarm-status" style="display:none">
    <span class="swarm-pulse"></span>
    <span class="swarm-label">Swarm active</span>
    <span class="swarm-detail" id="squad-swarm-detail"></span>
  </div>
  <div class="squad-detail-messages">
    <h3>Squad Messages</h3>
    <div class="squad-message-feed" id="squad-message-feed">
      <div class="squad-messages-empty">No messages yet. Squad members can broadcast messages using the squad_broadcast tool.</div>
    </div>
  </div>
  <div class="squad-detail-handoffs">
    <h3>Handoffs</h3>
    <div class="squad-handoff-feed" id="squad-handoff-feed">
      <div class="squad-messages-empty">No handoffs yet. Agents use the handoff channel to pass work to each other.</div>
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

// ── Agent Handoff Helpers ─────────────────────────────────────────────

/** Filter messages to only handoff-channel messages. */
export function filterHandoffs(messages: EngineAgentMessage[]): EngineAgentMessage[] {
  return messages.filter((m) => m.channel === 'handoff');
}

/** Render a single squad message card (broadcast or direct). */
export function renderSquadMessageCard(msg: EngineAgentMessage): string {
  const time = msg.created_at ? new Date(msg.created_at).toLocaleTimeString() : '';
  return `<div class="squad-msg-card${msg.read ? '' : ' unread'}">
    <div class="squad-msg-header">
      <span class="squad-msg-author">${escHtml(msg.from_agent)}</span>
      <span class="squad-msg-channel">#${escHtml(msg.channel)}</span>
      <span class="squad-msg-time">${escHtml(time)}</span>
    </div>
    <div class="squad-msg-body">${escHtml(msg.content)}</div>
  </div>`;
}

/** Render a single handoff message card. */
export function renderHandoffCard(msg: EngineAgentMessage): string {
  const meta = msg.metadata ? tryParseJson(msg.metadata) : null;
  const filesHtml = meta?.files
    ? `<div class="handoff-files">${(meta.files as string[]).map((f: string) => `<span class="handoff-file">${escHtml(f)}</span>`).join('')}</div>`
    : '';
  return `<div class="handoff-card${msg.read ? '' : ' unread'}">
    <div class="handoff-header">
      <span class="handoff-from">${escHtml(msg.from_agent)}</span>
      <span class="handoff-arrow">→</span>
      <span class="handoff-to">${escHtml(msg.to_agent)}</span>
    </div>
    <div class="handoff-content">${escHtml(msg.content)}</div>
    ${filesHtml}
  </div>`;
}

function tryParseJson(s: string): Record<string, unknown> | null {
  try {
    return JSON.parse(s) as Record<string, unknown>;
  } catch {
    return null;
  }
}
