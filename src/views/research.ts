// Research View — Agent-powered research
// Extracted from main.ts for maintainability

import { gateway } from '../gateway';
import { listProjects, saveProject, deleteProject, listDocs, saveDoc, deleteDoc } from '../db';
import type { ContentDoc } from '../db';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let wsConnected = false;
let _activeResearchId: string | null = null;
let _researchStreaming = false;
let _researchContent = '';
let _researchRunId: string | null = null;
let _researchResolve: ((text: string) => void) | null = null;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

// Expose streaming state for agent event routing in main.ts
export function isStreaming(): boolean {
  return _researchStreaming;
}

export function getRunId(): string | null {
  return _researchRunId;
}

export function appendDelta(text: string) {
  _researchContent += text;
  const liveContent = $('research-live-content');
  if (liveContent) {
    liveContent.textContent = _researchContent;
    liveContent.scrollTop = liveContent.scrollHeight;
  }
}

export function resolveStream(text?: string) {
  if (_researchResolve) {
    _researchResolve(text ?? _researchContent);
    _researchResolve = null;
  }
}

export function getContent(): string {
  return _researchContent;
}

export function setContent(text: string) {
  _researchContent = text;
}

// Callbacks
let promptModalFn: ((title: string, placeholder?: string) => Promise<string | null>) | null = null;

export function configure(opts: {
  promptModal: (title: string, placeholder?: string) => Promise<string | null>;
}) {
  promptModalFn = opts.promptModal;
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function showToast(message: string, type: 'info' | 'success' | 'error' | 'warning' = 'info', durationMs = 3500) {
  const existing = document.querySelector('.toast-notification');
  if (existing) existing.remove();
  const toast = document.createElement('div');
  toast.className = `toast-notification toast-${type}`;
  toast.textContent = message;
  document.body.appendChild(toast);
  requestAnimationFrame(() => toast.classList.add('show'));
  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 300);
  }, durationMs);
}

function formatResearchContent(text: string): string {
  return escHtml(text)
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/^[-•] (.+)$/gm, '<div class="research-bullet">• $1</div>')
    .replace(/\n/g, '<br>');
}

// ── Main functions ─────────────────────────────────────────────────────────
export async function loadResearchProjects() {
  const list = $('research-project-list');
  const empty = $('research-empty');
  const workspace = $('research-workspace');
  if (!list) return;

  const projects = await listProjects('research');
  list.innerHTML = '';

  if (!projects.length && !_activeResearchId) {
    if (empty) empty.style.display = 'flex';
    if (workspace) workspace.style.display = 'none';
    return;
  }

  for (const p of projects) {
    const item = document.createElement('div');
    item.className = `studio-doc-item${p.id === _activeResearchId ? ' active' : ''}`;
    item.innerHTML = `
      <div class="studio-doc-title">${escHtml(p.name)}</div>
      <div class="studio-doc-meta">${new Date(p.updated_at).toLocaleDateString()}</div>
    `;
    item.addEventListener('click', () => openResearchProject(p.id));
    list.appendChild(item);
  }
}

async function openResearchProject(id: string) {
  _activeResearchId = id;
  const empty = $('research-empty');
  const workspace = $('research-workspace');
  if (empty) empty.style.display = 'none';
  if (workspace) workspace.style.display = '';
  await loadResearchFindings(id);
  loadResearchProjects();
}

async function loadResearchFindings(projectId: string) {
  const list = $('research-findings-list');
  const header = $('research-findings-header');
  if (!list) return;

  const allDocs = await listDocs();
  const findings = allDocs.filter(d => d.project_id === projectId && d.content_type === 'research-finding');
  const savedReports = allDocs.filter(d => d.project_id === projectId && d.content_type === 'research-report');
  list.innerHTML = '';

  if (savedReports.length) {
    const reportBtn = document.createElement('button');
    reportBtn.className = 'btn btn-ghost btn-sm';
    reportBtn.style.marginBottom = '8px';
    reportBtn.textContent = `View saved report (${new Date(savedReports[0].created_at).toLocaleDateString()})`;
    reportBtn.addEventListener('click', () => {
      const reportArea = $('research-report-area');
      const findingsArea = $('research-findings-area');
      const reportContent = $('research-report-content');
      if (reportArea) reportArea.style.display = '';
      if (findingsArea) findingsArea.style.display = 'none';
      if (reportContent) reportContent.innerHTML = formatResearchContent(savedReports[0].content);
    });
    list.appendChild(reportBtn);
  }

  if (findings.length) {
    if (header) header.style.display = 'flex';
    for (const f of findings) {
      list.appendChild(renderFindingCard(f));
    }
  } else {
    if (header) header.style.display = 'none';
  }
}

function renderFindingCard(doc: ContentDoc): HTMLElement {
  const card = document.createElement('div');
  card.className = 'research-finding-card';
  card.innerHTML = `
    <div class="research-finding-header">
      <div class="research-finding-title">${escHtml(doc.title)}</div>
      <div class="research-finding-actions">
        <span class="research-finding-meta">${new Date(doc.created_at).toLocaleString()}</span>
        <button class="btn btn-ghost btn-xs research-finding-delete" data-id="${escAttr(doc.id)}" title="Remove">✕</button>
      </div>
    </div>
    <div class="research-finding-body">${formatResearchContent(doc.content)}</div>
  `;
  card.querySelector('.research-finding-delete')?.addEventListener('click', async (e) => {
    e.stopPropagation();
    await deleteDoc(doc.id);
    if (_activeResearchId) loadResearchFindings(_activeResearchId);
  });
  return card;
}

async function runResearch() {
  if (!_activeResearchId || !wsConnected || _researchStreaming) return;
  const input = $('research-topic-input') as HTMLInputElement | null;
  const topic = input?.value.trim();
  if (!topic) return;

  const projectId = _activeResearchId;
  const sessionKey = 'paw-research-' + projectId;

  _researchStreaming = true;
  _researchContent = '';
  _researchRunId = null;
  const liveArea = $('research-live');
  const liveContent = $('research-live-content');
  const runBtn = $('research-run-btn');
  if (liveArea) liveArea.style.display = '';
  if (liveContent) liveContent.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';
  if (runBtn) runBtn.setAttribute('disabled', 'true');
  const label = $('research-live-label');
  if (label) label.textContent = 'Researching…';

  const done = new Promise<string>((resolve) => {
    _researchResolve = resolve;
    setTimeout(() => resolve(_researchContent || '(Research timed out)'), 180_000);
  });

  try {
    const result = await gateway.chatSend(sessionKey,
      `Research this topic thoroughly. Browse the web, find multiple sources, and provide detailed findings with key insights, data points, and source URLs. Be comprehensive and structured.\n\nTopic: ${topic}`
    );
    if (result.runId) _researchRunId = result.runId;

    const finalText = await done;

    const findingId = crypto.randomUUID();
    await saveDoc({
      id: findingId,
      project_id: projectId,
      title: topic,
      content: finalText,
      content_type: 'research-finding',
    });

    if (liveArea) liveArea.style.display = 'none';
    if (input) input.value = '';
    await loadResearchFindings(projectId);
  } catch (e) {
    console.error('[research] Error:', e);
    if (liveContent) {
      liveContent.textContent = `Error: ${e instanceof Error ? e.message : e}`;
    }
  } finally {
    _researchStreaming = false;
    _researchRunId = null;
    _researchResolve = null;
    if (runBtn) runBtn.removeAttribute('disabled');
    if (label) label.textContent = 'Done';
  }
}

async function generateResearchReport() {
  if (!_activeResearchId || !wsConnected) return;

  const allDocs = await listDocs();
  const findings = allDocs.filter(d => d.project_id === _activeResearchId && d.content_type === 'research-finding');
  if (!findings.length) { alert('No findings yet — run some research first'); return; }

  const reportArea = $('research-report-area');
  const findingsArea = $('research-findings-area');
  const reportContent = $('research-report-content');
  if (reportArea) reportArea.style.display = '';
  if (findingsArea) findingsArea.style.display = 'none';
  if (reportContent) reportContent.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';

  const findingsText = findings.map((f, i) => `## Finding ${i + 1}: ${f.title}\n${f.content}`).join('\n\n---\n\n');
  const sessionKey = 'paw-research-' + _activeResearchId;

  const prevStreaming = _researchStreaming;
  _researchStreaming = true;
  _researchContent = '';

  const done = new Promise<string>((resolve) => {
    _researchResolve = resolve;
    setTimeout(() => resolve(_researchContent || '(Report generation timed out)'), 180_000);
  });

  try {
    const result = await gateway.chatSend(sessionKey,
      `Based on all the research findings below, write a comprehensive, well-structured report. Include an executive summary, key findings organized by theme, conclusions, and a list of sources. Use markdown formatting.\n\n${findingsText}`
    );
    if (result.runId) _researchRunId = result.runId;

    const reportText = await done;
    if (reportContent) reportContent.innerHTML = formatResearchContent(reportText);

    if (reportText && _activeResearchId) {
      const reportId = crypto.randomUUID();
      await saveDoc({
        id: reportId,
        project_id: _activeResearchId,
        title: `Research Report — ${new Date().toLocaleDateString()}`,
        content: reportText,
        content_type: 'research-report',
      });
      showToast('Report saved', 'success');
    }
  } catch (e) {
    if (reportContent) reportContent.textContent = `Error generating report: ${e instanceof Error ? e.message : e}`;
  } finally {
    _researchStreaming = prevStreaming;
    _researchRunId = null;
    _researchResolve = null;
  }
}

async function createNewResearch() {
  const name = await promptModalFn?.('Research project name:', 'My research project');
  if (!name) return;
  const id = crypto.randomUUID();
  await saveProject({ id, name, space: 'research' });
  openResearchProject(id);
  loadResearchProjects();
}

// ── Event wiring ───────────────────────────────────────────────────────────
export function initResearchEvents() {
  $('research-new-project')?.addEventListener('click', createNewResearch);
  $('research-create-first')?.addEventListener('click', createNewResearch);

  $('research-run-btn')?.addEventListener('click', runResearch);
  $('research-topic-input')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); runResearch(); }
  });

  $('research-abort-btn')?.addEventListener('click', () => {
    if (!_activeResearchId) return;
    gateway.chatAbort('paw-research-' + _activeResearchId).catch(console.warn);
    if (_researchResolve) {
      _researchResolve(_researchContent || '(Aborted)');
      _researchResolve = null;
    }
  });

  $('research-generate-report')?.addEventListener('click', generateResearchReport);

  $('research-close-report')?.addEventListener('click', () => {
    const reportArea = $('research-report-area');
    const findingsArea = $('research-findings-area');
    if (reportArea) reportArea.style.display = 'none';
    if (findingsArea) findingsArea.style.display = '';
  });

  $('research-delete-project')?.addEventListener('click', async () => {
    if (!_activeResearchId) return;
    if (!confirm('Delete this research project and all its findings?')) return;
    const allDocs = await listDocs();
    for (const d of allDocs.filter(d => d.project_id === _activeResearchId)) {
      await deleteDoc(d.id);
    }
    await deleteProject(_activeResearchId);
    _activeResearchId = null;
    const workspace = $('research-workspace');
    const empty = $('research-empty');
    if (workspace) workspace.style.display = 'none';
    if (empty) empty.style.display = 'flex';
    loadResearchProjects();
  });
}
