import { describe, it, expect } from 'vitest';
import { renderSquadCard, renderSquadDetail, buildAgentOptions } from './atoms';
import type { EngineSquad, EngineSquadMember } from '../../engine/atoms/types';

// ── helpers ────────────────────────────────────────────────────────────

function makeSquad(overrides: Partial<EngineSquad> = {}): EngineSquad {
  return {
    id: 'sq-1',
    name: 'Alpha Squad',
    goal: 'Ship the MVP',
    status: 'active',
    members: [],
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}

function makeMember(overrides: Partial<EngineSquadMember> = {}): EngineSquadMember {
  return { agent_id: 'agent-1', role: 'member', ...overrides };
}

// ── renderSquadCard ────────────────────────────────────────────────────

describe('renderSquadCard', () => {
  it('renders card with squad name', () => {
    const html = renderSquadCard(makeSquad(), false);
    expect(html).toContain('Alpha Squad');
  });

  it('includes active class when selected', () => {
    const html = renderSquadCard(makeSquad(), true);
    expect(html).toContain('squad-card active');
  });

  it('omits active class when not selected', () => {
    const html = renderSquadCard(makeSquad(), false);
    expect(html).not.toContain('squad-card active');
    expect(html).toContain('squad-card');
  });

  it('shows member count singular', () => {
    const squad = makeSquad({ members: [makeMember()] });
    const html = renderSquadCard(squad, false);
    expect(html).toContain('1 member');
    expect(html).not.toContain('1 members');
  });

  it('shows member count plural', () => {
    const members = [makeMember(), makeMember({ agent_id: 'agent-2' })];
    const html = renderSquadCard(makeSquad({ members }), false);
    expect(html).toContain('2 members');
  });

  it('shows status badge', () => {
    const html = renderSquadCard(makeSquad({ status: 'paused' }), false);
    expect(html).toContain('paused');
  });

  it('sets data-squad-id attribute', () => {
    const html = renderSquadCard(makeSquad({ id: 'my-squad' }), false);
    expect(html).toContain('data-squad-id="my-squad"');
  });

  it('shows goal text', () => {
    const html = renderSquadCard(makeSquad({ goal: 'Test goal' }), false);
    expect(html).toContain('Test goal');
  });

  it('shows fallback when goal is empty', () => {
    const html = renderSquadCard(makeSquad({ goal: '' }), false);
    expect(html).toContain('No goal set');
  });

  it('escapes HTML in name to prevent XSS', () => {
    const html = renderSquadCard(makeSquad({ name: '<script>alert(1)</script>' }), false);
    expect(html).not.toContain('<script>');
    expect(html).toContain('&lt;script&gt;');
  });
});

// ── renderSquadDetail ──────────────────────────────────────────────────

describe('renderSquadDetail', () => {
  it('renders detail with squad name heading', () => {
    const html = renderSquadDetail(makeSquad());
    expect(html).toContain('Alpha Squad');
    expect(html).toContain('squad-detail-name');
  });

  it('renders edit and delete buttons', () => {
    const html = renderSquadDetail(makeSquad());
    expect(html).toContain('id="squad-edit-btn"');
    expect(html).toContain('id="squad-delete-btn"');
  });

  it('renders add member button', () => {
    const html = renderSquadDetail(makeSquad());
    expect(html).toContain('id="squad-add-member-btn"');
  });

  it('renders member rows with roles', () => {
    const members = [
      makeMember({ agent_id: 'coder-1', role: 'coordinator' }),
      makeMember({ agent_id: 'researcher-1', role: 'member' }),
    ];
    const html = renderSquadDetail(makeSquad({ members }));
    expect(html).toContain('coder-1');
    expect(html).toContain('researcher-1');
    expect(html).toContain('coordinator');
  });

  it('shows empty state when no members', () => {
    const html = renderSquadDetail(makeSquad({ members: [] }));
    expect(html).toContain('No members yet');
  });

  it('renders remove buttons with agent ids', () => {
    const members = [makeMember({ agent_id: 'ag-99' })];
    const html = renderSquadDetail(makeSquad({ members }));
    expect(html).toContain('squad-remove-member');
    expect(html).toContain('data-agent-id="ag-99"');
  });

  it('renders goal section', () => {
    const html = renderSquadDetail(makeSquad({ goal: 'Build features' }));
    expect(html).toContain('Build features');
  });

  it('escapes HTML in goal', () => {
    const html = renderSquadDetail(makeSquad({ goal: '<img onerror=alert(1)>' }));
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });

  it('renders message feed section', () => {
    const html = renderSquadDetail(makeSquad());
    expect(html).toContain('squad-message-feed');
    expect(html).toContain('Squad Messages');
  });
});

// ── buildAgentOptions ──────────────────────────────────────────────────

describe('buildAgentOptions', () => {
  const agents = [
    { id: 'a1', name: 'Alice' },
    { id: 'a2', name: 'Bob' },
    { id: 'a3', name: 'Charlie' },
  ];

  it('returns options for all agents when no existing members', () => {
    const html = buildAgentOptions(agents, []);
    expect(html).toContain('Alice');
    expect(html).toContain('Bob');
    expect(html).toContain('Charlie');
  });

  it('excludes agents already in squad', () => {
    const existing = [makeMember({ agent_id: 'a2' })];
    const html = buildAgentOptions(agents, existing);
    expect(html).toContain('Alice');
    expect(html).not.toContain('Bob');
    expect(html).toContain('Charlie');
  });

  it('returns empty string when all agents are members', () => {
    const existing = agents.map((a) => makeMember({ agent_id: a.id }));
    const html = buildAgentOptions(agents, existing);
    expect(html).toBe('');
  });

  it('returns empty string for empty agent list', () => {
    const html = buildAgentOptions([], []);
    expect(html).toBe('');
  });

  it('includes value attribute with agent id', () => {
    const html = buildAgentOptions([{ id: 'x1', name: 'X' }], []);
    expect(html).toContain('value="x1"');
  });

  it('escapes HTML in agent names', () => {
    const html = buildAgentOptions([{ id: 'x', name: '<b>Bad</b>' }], []);
    expect(html).not.toContain('<b>');
    expect(html).toContain('&lt;b&gt;');
  });
});
