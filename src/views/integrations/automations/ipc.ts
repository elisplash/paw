// src/views/integrations/automations/ipc.ts — IPC helpers for automation commands
//
// Atom-level: thin invoke wrappers, no DOM.

import { invoke } from '@tauri-apps/api/core';
import type { AutomationTemplate, ActiveAutomation } from './atoms';
import { SERVICE_CATALOG } from '../catalog';

// ── Service lookup helpers ─────────────────────────────────────────────

export function svcName(id: string): string {
  return SERVICE_CATALOG.find((s) => s.id === id)?.name ?? id;
}

export function svcIcon(id: string): string {
  return SERVICE_CATALOG.find((s) => s.id === id)?.icon ?? 'extension';
}

export function svcColor(id: string): string {
  return SERVICE_CATALOG.find((s) => s.id === id)?.color ?? '#888';
}

// ── IPC calls ──────────────────────────────────────────────────────────

export async function activateTemplate(tpl: AutomationTemplate): Promise<ActiveAutomation> {
  return invoke<ActiveAutomation>('engine_automations_activate_template', {
    templateId: tpl.id,
    template: tpl,
  });
}

export async function toggleAutomation(id: string, action: 'pause' | 'resume'): Promise<void> {
  await invoke('engine_automations_toggle', { automationId: id, action });
}

export async function deleteAutomation(id: string): Promise<void> {
  await invoke('engine_automations_delete', { automationId: id });
}

export async function listAutomations(): Promise<ActiveAutomation[]> {
  return invoke<ActiveAutomation[]>('engine_automations_list');
}

// ── Date formatting ────────────────────────────────────────────────────

export function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return 'just now';
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    return d.toLocaleDateString();
  } catch {
    return iso;
  }
}
