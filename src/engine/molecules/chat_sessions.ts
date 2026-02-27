// src/engine/molecules/chat_sessions.ts
// Session management molecule — load, render, switch, history.
// Extracted from chat_controller.ts to respect atomic boundaries.

import { pawEngine } from '../../engine';
import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
  groupSessionMap,
} from '../../state/index';
import { escHtml } from '../../components/helpers';
import * as AgentsModule from '../../views/agents';

// ── Types ────────────────────────────────────────────────────────────────

/** Organism-level callbacks the session manager needs but must not import. */
export interface SessionManagerDeps {
  teardownStream: (key: string, reason: string) => void;
  resetTokenMeter: () => void;
  renderMessages: () => void;
}

export interface SessionManager {
  loadSessions: (opts?: { skipHistory?: boolean }) => Promise<void>;
  renderSessionSelect: () => void;
  populateAgentSelect: () => void;
  switchToAgent: (agentId: string) => Promise<void>;
  loadChatHistory: (sessionKey: string) => Promise<void>;
}

// ── DOM shorthand ────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);

// ── Factory ──────────────────────────────────────────────────────────────

export function createSessionManager(deps: SessionManagerDeps): SessionManager {
  // ── Load sessions ────────────────────────────────────────────────────

  async function loadSessions(opts?: { skipHistory?: boolean }): Promise<void> {
    if (!appState.wsConnected) return;
    try {
      const engineSessions = await pawEngine.sessionsList(200);

      // Auto-prune empty sessions older than 1 hour
      pawEngine
        .sessionCleanup(3600, appState.currentSessionKey ?? undefined)
        .then((n) => {
          if (n > 0) console.debug(`[chat] Pruned ${n} empty session(s)`);
        })
        .catch((e) => console.warn('[chat] Session cleanup failed:', e));

      const ONE_HOUR = 60 * 60 * 1000;
      const now = Date.now();
      const keptSessions = engineSessions.filter((s) => {
        const age = s.updated_at ? now - new Date(s.updated_at).getTime() : Infinity;
        const isEmpty = s.message_count === 0;
        const isCurrentSession = s.id === appState.currentSessionKey;
        return !(isEmpty && age > ONE_HOUR && !isCurrentSession);
      });

      appState.sessions = keptSessions.map((s) => {
        const gm = groupSessionMap.get(s.id);
        return {
          key: s.id,
          kind: gm ? ('group' as const) : ('direct' as const),
          label: s.label ?? undefined,
          displayName: s.label ?? s.id,
          updatedAt: s.updated_at ? new Date(s.updated_at).getTime() : undefined,
          agentId: s.agent_id ?? undefined,
          members: gm?.members,
        };
      });

      // Re-add pending group sessions (not yet backed by a real session) from localStorage
      for (const [pendingKey, gm] of groupSessionMap) {
        if (
          pendingKey.startsWith('pending-group_') &&
          !appState.sessions.some((s) => s.key === pendingKey)
        ) {
          appState.sessions.unshift({
            key: pendingKey,
            kind: 'group',
            agentId: undefined,
            label: gm.name,
            displayName: gm.name,
            members: gm.members,
            updatedAt: Date.now(),
          });
        }
      }

      const currentAgent = AgentsModule.getCurrentAgent();
      if (!appState.currentSessionKey && currentAgent) {
        const savedKey = agentSessionMap.get(currentAgent.id);
        const isValidSaved =
          savedKey &&
          appState.sessions.some(
            (s) =>
              s.key === savedKey &&
              (s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId)),
          );
        if (isValidSaved) {
          appState.currentSessionKey = savedKey;
        } else {
          const agentSession = appState.sessions.find(
            (s) => s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId),
          );
          if (agentSession) {
            appState.currentSessionKey = agentSession.key;
            agentSessionMap.set(currentAgent.id, agentSession.key);
            persistAgentSessionMap();
          }
        }
      } else if (!appState.currentSessionKey && appState.sessions.length) {
        appState.currentSessionKey = appState.sessions[0].key;
      }

      renderSessionSelect();
      const sessionBusy = appState.activeStreams.has(appState.currentSessionKey ?? '');
      if (!opts?.skipHistory && appState.currentSessionKey && !sessionBusy) {
        await loadChatHistory(appState.currentSessionKey);
      }
    } catch (e) {
      console.warn('[chat] Sessions load failed:', e);
    }
  }

  // ── Render session dropdown ──────────────────────────────────────────

  function renderSessionSelect(): void {
    const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
    if (!chatSessionSelect) return;
    chatSessionSelect.innerHTML = '';

    const currentAgent = AgentsModule.getCurrentAgent();
    const agentSessions = currentAgent
      ? appState.sessions.filter(
          (s) => s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId),
        )
      : appState.sessions;

    if (!agentSessions.length) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = 'No sessions — send a message to start';
      chatSessionSelect.appendChild(opt);
      return;
    }

    const MAX_SESSIONS = 25;
    const sorted = [...agentSessions].sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
    const limited = sorted.slice(0, MAX_SESSIONS);

    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const yesterdayStart = new Date(todayStart);
    yesterdayStart.setDate(yesterdayStart.getDate() - 1);
    const weekStart = new Date(todayStart);
    weekStart.setDate(weekStart.getDate() - 7);

    const groups: { label: string; sessions: typeof limited }[] = [
      { label: 'Today', sessions: [] },
      { label: 'Yesterday', sessions: [] },
      { label: 'This Week', sessions: [] },
      { label: 'Older', sessions: [] },
    ];

    for (const s of limited) {
      const t = s.updatedAt ?? 0;
      if (t >= todayStart.getTime()) groups[0].sessions.push(s);
      else if (t >= yesterdayStart.getTime()) groups[1].sessions.push(s);
      else if (t >= weekStart.getTime()) groups[2].sessions.push(s);
      else groups[3].sessions.push(s);
    }

    for (const g of groups) {
      if (!g.sessions.length) continue;
      const optgroup = document.createElement('optgroup');
      optgroup.label = g.label;
      for (const s of g.sessions) {
        const opt = document.createElement('option');
        opt.value = s.key;
        const raw = s.label ?? s.displayName ?? 'Untitled chat';
        const label = raw.length > 40 ? `${raw.slice(0, 37)}…` : raw;
        opt.textContent = label;
        opt.title = raw;
        if (s.key === appState.currentSessionKey) opt.selected = true;
        optgroup.appendChild(opt);
      }
      chatSessionSelect.appendChild(optgroup);
    }

    if (sorted.length > MAX_SESSIONS) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.disabled = true;
      opt.textContent = `… ${sorted.length - MAX_SESSIONS} older sessions`;
      chatSessionSelect.appendChild(opt);
    }
  }

  // ── Populate agent dropdown ──────────────────────────────────────────

  function populateAgentSelect(): void {
    const chatAgentSelect = $('chat-agent-select') as HTMLSelectElement | null;
    if (!chatAgentSelect) return;
    const agents = AgentsModule.getAgents();
    const currentAgent = AgentsModule.getCurrentAgent();
    chatAgentSelect.innerHTML = '';
    for (const a of agents) {
      const opt = document.createElement('option');
      opt.value = a.id;
      opt.textContent = a.name;
      if (a.id === currentAgent?.id) opt.selected = true;
      chatAgentSelect.appendChild(opt);
    }
  }

  // ── Switch agent ─────────────────────────────────────────────────────

  async function switchToAgent(agentId: string): Promise<void> {
    const prevAgent = AgentsModule.getCurrentAgent();
    if (prevAgent && appState.currentSessionKey) {
      agentSessionMap.set(prevAgent.id, appState.currentSessionKey);
      persistAgentSessionMap();
    }

    const oldKey = appState.currentSessionKey ?? '';
    deps.teardownStream(oldKey, 'Agent switched');

    AgentsModule.setSelectedAgent(agentId);
    const agent = AgentsModule.getCurrentAgent();
    const chatAgentName = $('chat-agent-name');
    if (chatAgentName && agent) {
      chatAgentName.innerHTML = `${AgentsModule.spriteAvatar(agent.avatar, 20)} ${escHtml(agent.name)}`;
    }
    const chatAvatarEl = document.getElementById('chat-avatar');
    if (chatAvatarEl && agent) {
      chatAvatarEl.innerHTML = AgentsModule.spriteAvatar(agent.avatar, 32);
    }
    const chatAgentSelect = $('chat-agent-select') as HTMLSelectElement | null;
    if (chatAgentSelect) chatAgentSelect.value = agentId;

    deps.resetTokenMeter();

    const savedSessionKey = agentSessionMap.get(agentId);
    const savedSessionValid =
      savedSessionKey &&
      appState.sessions.some(
        (s) =>
          s.key === savedSessionKey &&
          (s.agentId === agentId || (agentId === 'default' && !s.agentId)),
      );
    if (savedSessionValid) {
      appState.currentSessionKey = savedSessionKey;
      renderSessionSelect();
      await loadChatHistory(savedSessionKey);
      const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
      if (chatSessionSelect) chatSessionSelect.value = savedSessionKey;
    } else {
      const agentSession = appState.sessions.find(
        (s) => s.agentId === agentId || (agentId === 'default' && !s.agentId),
      );
      if (agentSession) {
        appState.currentSessionKey = agentSession.key;
        agentSessionMap.set(agentId, agentSession.key);
        persistAgentSessionMap();
        renderSessionSelect();
        await loadChatHistory(agentSession.key);
        const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
        if (chatSessionSelect) chatSessionSelect.value = agentSession.key;
      } else {
        appState.currentSessionKey = null;
        appState.messages = [];
        renderSessionSelect();
        deps.renderMessages();
        const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
        if (chatSessionSelect) chatSessionSelect.value = '';
      }
    }
    console.debug(
      `[chat] Switched to agent "${agent?.name}" (${agentId}), session=${appState.currentSessionKey ?? 'new'}`,
    );
  }

  // ── Load chat history ────────────────────────────────────────────────

  async function loadChatHistory(sessionKey: string): Promise<void> {
    if (!appState.wsConnected) return;
    try {
      const stored = await pawEngine.chatHistory(sessionKey, 200);
      appState.messages = stored
        .filter((m) => m.role === 'user' || m.role === 'assistant')
        .map((m) => ({
          id: m.id,
          role: m.role as 'user' | 'assistant' | 'system',
          content: m.content,
          timestamp: new Date(m.created_at),
        }));
      deps.renderMessages();
    } catch (e) {
      console.warn('[chat] History load failed:', e);
      appState.messages = [];
      deps.renderMessages();
    }
  }

  return { loadSessions, renderSessionSelect, populateAgentSelect, switchToAgent, loadChatHistory };
}
