// src/views/integrations/queries/index.ts — Orchestration + public API
//
// Thin barrel: state bridge, re-exports.

import { renderQueryPanel, renderServiceQueries, initQueryMoleculesState } from './molecules';

// ── Module state ───────────────────────────────────────────────────────

let _connectedIds: Set<string> = new Set();

const { setQueryMoleculesState } = initQueryMoleculesState();
setQueryMoleculesState({
  getConnectedIds: () => _connectedIds,
});

// ── Public API ─────────────────────────────────────────────────────────

export function setQueryConnectedIds(ids: Set<string>): void {
  _connectedIds = ids;
}

export function loadQueryPanel(container: HTMLElement): void {
  renderQueryPanel(container);
}

export function loadServiceQueries(container: HTMLElement, serviceId: string): void {
  renderServiceQueries(container, serviceId);
}

export { QUERY_CATALOG } from './catalog';
export type { ServiceQuery } from './atoms';
