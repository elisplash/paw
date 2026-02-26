// src/views/integrations/queries/molecules.ts — Query browse & click-to-chat
//
// Molecule-level: DOM rendering, event wiring.

import {
  filterQueries,
  isQueryReady,
  QUERY_CATEGORIES,
  type ServiceQuery,
  type QueryCategory,
} from './atoms';
import { QUERY_CATALOG, getQueriesForService } from './catalog';
import { svcName, svcIcon, svcColor } from './ipc';
import { escHtml } from '../atoms';
import { sendMessage } from '../../../engine/organisms/chat_controller';

// ── Module state ───────────────────────────────────────────────────────

interface MoleculesState {
  getConnectedIds: () => Set<string>;
}

let _state: MoleculesState = {
  getConnectedIds: () => new Set(),
};

export function initQueryMoleculesState(): {
  setQueryMoleculesState: (s: MoleculesState) => void;
} {
  return {
    setQueryMoleculesState: (s) => {
      _state = s;
    },
  };
}

// ── Filter state ───────────────────────────────────────────────────────

let _searchQuery = '';
let _activeCategory: QueryCategory | 'all' = 'all';

// ── Send to chat ───────────────────────────────────────────────────────

function _sendToChat(question: string): void {
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  if (chatInput) {
    chatInput.value = question;
    chatInput.style.height = 'auto';
  }
  sendMessage();
}

// ── Main panel render ──────────────────────────────────────────────────

export function renderQueryPanel(container: HTMLElement): void {
  const connectedIds = _state.getConnectedIds();
  const filtered = filterQueries(QUERY_CATALOG, {
    category: _activeCategory,
    query: _searchQuery,
  });

  const readyCount = filtered.filter((q) => isQueryReady(q, connectedIds)).length;

  container.innerHTML = `
    <div class="queries-header">
      <h2><span class="ms ms-lg">psychology</span> Ask Your Agent</h2>
      <p class="queries-subtitle">
        ${QUERY_CATALOG.length} queries · ${readyCount} ready
      </p>
    </div>

    <div class="queries-toolbar">
      <div class="queries-search-wrap">
        <span class="ms ms-sm">search</span>
        <input type="text" class="queries-search" id="queries-search"
               placeholder="Search queries…" value="${escHtml(_searchQuery)}" />
      </div>
    </div>

    <div class="queries-cat-pills" id="queries-cat-pills">
      <button class="integrations-cat-pill ${_activeCategory === 'all' ? 'active' : ''}"
              data-cat="all">All</button>
      ${QUERY_CATEGORIES.map(
        (c) => `
        <button class="integrations-cat-pill ${_activeCategory === c.id ? 'active' : ''}"
                data-cat="${c.id}">
          <span class="ms ms-sm">${c.icon}</span>${c.label}
        </button>
      `,
      ).join('')}
    </div>

    <div class="queries-grid" id="queries-grid">
      ${
        filtered.length === 0
          ? `<div class="integrations-empty">
            <span class="ms ms-lg">search_off</span>
            <p>No queries match your search</p>
          </div>`
          : filtered.map((q) => _renderQueryCard(q, connectedIds)).join('')
      }
    </div>
  `;

  _wireEvents(container);
}

// ── Query card ─────────────────────────────────────────────────────────

function _renderQueryCard(q: ServiceQuery, connectedIds: Set<string>): string {
  const ready = isQueryReady(q, connectedIds);

  return `
    <div class="query-card ${ready ? 'ready' : 'not-ready'}" data-query-id="${q.id}">
      <div class="query-card-header">
        <span class="query-card-icon"><span class="ms ms-sm">${q.icon}</span></span>
        <div class="query-card-services">
          ${q.serviceIds
            .map(
              (sid) => `
            <span class="query-svc-dot ${connectedIds.has(sid) ? 'connected' : 'missing'}"
                  style="color: ${svcColor(sid)}" title="${svcName(sid)}">
              <span class="ms ms-xs">${svcIcon(sid)}</span>
            </span>
          `,
            )
            .join('')}
        </div>
      </div>
      <p class="query-card-question">${escHtml(q.question)}</p>
      <p class="query-card-hint">${escHtml(q.resultHint)}</p>
      <div class="query-card-footer">
        ${
          ready
            ? `<button class="btn btn-primary btn-sm query-ask-btn" data-query-id="${q.id}">
              <span class="ms ms-sm">send</span> Ask
            </button>`
            : `<span class="query-connect-hint">
              <span class="ms ms-sm">link_off</span>
              Connect ${q.serviceIds
                .filter((s) => !connectedIds.has(s))
                .map(svcName)
                .join(', ')}
            </span>`
        }
      </div>
    </div>
  `;
}

// ── Events ─────────────────────────────────────────────────────────────

function _wireEvents(container: HTMLElement): void {
  // Search
  const search = container.querySelector('#queries-search') as HTMLInputElement;
  search?.addEventListener('input', () => {
    _searchQuery = search.value;
    renderQueryPanel(container);
  });

  // Category pills
  container.querySelector('#queries-cat-pills')?.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.integrations-cat-pill') as HTMLElement;
    if (!btn) return;
    _activeCategory = (btn.dataset.cat ?? 'all') as QueryCategory | 'all';
    renderQueryPanel(container);
  });

  // Ask buttons → send to chat
  container.querySelectorAll('.query-ask-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const qid = (btn as HTMLElement).dataset.queryId;
      const query = QUERY_CATALOG.find((q) => q.id === qid);
      if (query) _sendToChat(query.question);
    });
  });

  // Clicking the card also sends (if ready)
  container.querySelectorAll('.query-card.ready').forEach((card) => {
    card.addEventListener('click', () => {
      const qid = (card as HTMLElement).dataset.queryId;
      const query = QUERY_CATALOG.find((q) => q.id === qid);
      if (query) _sendToChat(query.question);
    });
  });
}

// ── Render for service detail panel ────────────────────────────────────

export function renderServiceQueries(container: HTMLElement, serviceId: string): void {
  const queries = getQueriesForService(serviceId);
  const connectedIds = _state.getConnectedIds();

  if (queries.length === 0) {
    container.innerHTML = `
      <div class="query-svc-empty">
        <p>No example queries for this service yet.</p>
      </div>
    `;
    return;
  }

  container.innerHTML = `
    <div class="query-svc-list">
      ${queries
        .map(
          (q) => `
        <button class="query-example-chip ${isQueryReady(q, connectedIds) ? 'ready' : ''}"
                data-question="${escHtml(q.question)}"
                title="${escHtml(q.resultHint)}">
          <span class="ms ms-sm">${q.icon}</span>
          <span class="query-example-text">"${escHtml(q.question)}"</span>
          <span class="ms ms-sm query-example-send">send</span>
        </button>
      `,
        )
        .join('')}
    </div>
  `;

  // Click to send
  container.querySelectorAll('.query-example-chip.ready').forEach((chip) => {
    chip.addEventListener('click', () => {
      const question = (chip as HTMLElement).dataset.question;
      if (question) _sendToChat(question);
    });
  });
}
