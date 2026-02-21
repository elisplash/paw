// Research View — Molecules (DOM rendering + IPC)

import { pawEngine } from '../../engine';
import * as workspace from '../../workspace';
import type { ResearchFinding, ResearchSource } from '../../workspace';
import { $, escHtml, formatMarkdown, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import { extractDomain, buildResearchPrompt, modeTimeout, type ResearchMode } from './atoms';

// ── State accessors (set by index.ts) ──────────────────────────────────────

interface MoleculesState {
  getActiveProject: () => workspace.ResearchProject | null;
  getFindings: () => ResearchFinding[];
  setFindings: (f: ResearchFinding[]) => void;
  getIsResearching: () => boolean;
  setIsResearching: (v: boolean) => void;
  getResearchMode: () => ResearchMode;
  getStreamContent: () => string;
  setStreamContent: (s: string) => void;
  getStreamResolve: () => ((text: string) => void) | null;
  setStreamResolve: (fn: ((text: string) => void) | null) => void;
  getLiveSources: () => ResearchSource[];
  pushLiveSource: (s: ResearchSource) => void;
  getLiveSteps: () => string[];
  pushLiveStep: (s: string) => void;
  setRunId: (id: string | null) => void;
  resetLiveState: () => void;
  getPromptModal: () => ((title: string, placeholder?: string) => Promise<string | null>) | null;
  reloadProjects: () => Promise<void>;
}

let _state: MoleculesState;

export function setMoleculesState(s: MoleculesState) {
  _state = s;
}

// ── Live streaming UI ──────────────────────────────────────────────────────

export function renderLiveSourceFeed() {
  const feed = $('research-source-feed');
  if (!feed) return;

  feed.innerHTML = _state
    .getLiveSources()
    .slice(-8)
    .map(
      (source) => `
    <div class="research-live-source">
      <span class="research-live-source-icon"><span class="ms ms-sm">language</span></span>
      <span class="research-live-source-domain">${escHtml(source.title)}</span>
    </div>
  `,
    )
    .join('');
}

export function renderProgressSteps() {
  const container = $('research-progress-steps');
  if (!container) return;

  const steps = _state.getLiveSteps();
  container.innerHTML = steps
    .map(
      (step, i) => `
    <div class="research-progress-step ${i === steps.length - 1 ? 'active' : 'done'}">
      <span class="research-step-icon">${i === steps.length - 1 ? '◉' : '✓'}</span>
      <span class="research-step-text">${escHtml(step)}</span>
    </div>
  `,
    )
    .join('');
}

// ── Project list ───────────────────────────────────────────────────────────

export async function renderProjectList() {
  await workspace.ensureWorkspace();

  const list = $('research-project-list');
  const empty = $('research-empty');
  const main = $('research-workspace');
  if (!list) return;

  const projects = await workspace.listResearchProjects();
  list.innerHTML = '';
  const active = _state.getActiveProject();

  if (!projects.length && !active) {
    if (empty) empty.style.display = 'flex';
    if (main) main.style.display = 'none';
    return;
  }

  for (const p of projects) {
    const item = document.createElement('div');
    item.className = `research-project-item${p.id === active?.id ? ' active' : ''}`;
    item.innerHTML = `
      <div class="research-project-name">${escHtml(p.name)}</div>
      <div class="research-project-meta">
        <span>${p.queries.length} queries</span>
        <span>•</span>
        <span>${new Date(p.updated).toLocaleDateString()}</span>
      </div>
    `;
    item.addEventListener('click', () => openProject(p.id));
    list.appendChild(item);
  }

  // Recent queries section
  const recentQueries = projects
    .flatMap((p) =>
      p.queries.slice(-3).map((q) => ({ query: q, projectId: p.id, projectName: p.name })),
    )
    .slice(0, 5);

  const recentList = $('research-recent-queries');
  if (recentList && recentQueries.length) {
    recentList.innerHTML = recentQueries
      .map(
        (r) => `
      <div class="research-recent-query" data-project="${r.projectId}" data-query="${escHtml(r.query)}">
        <span class="research-recent-icon">↩</span>
        <span class="research-recent-text">${escHtml(r.query.slice(0, 40))}${r.query.length > 40 ? '...' : ''}</span>
      </div>
    `,
      )
      .join('');

    recentList.querySelectorAll('.research-recent-query').forEach((el) => {
      el.addEventListener('click', async () => {
        const projectId = el.getAttribute('data-project');
        const query = el.getAttribute('data-query');
        if (projectId && query) {
          await openProject(projectId);
          const input = $('research-topic-input') as HTMLInputElement;
          if (input) input.value = query;
        }
      });
    });
  }
}

// ── Open project ───────────────────────────────────────────────────────────

export async function openProject(id: string) {
  const project = await workspace.getResearchProject(id);
  if (!project) return;

  // Update state via index
  _state.setFindings(await workspace.listFindings(id));

  // Expose the project to index.ts — we need a special setter
  // The project is written directly into module state through the accessor
  // pattern: index.ts passes getActiveProject/setActiveProject
  (
    _state as unknown as { setActiveProject: (p: workspace.ResearchProject | null) => void }
  ).setActiveProject(project);

  const empty = $('research-empty');
  const main = $('research-workspace');
  if (empty) empty.style.display = 'none';
  if (main) main.style.display = '';

  // Update header
  const header = $('research-project-header');
  if (header) {
    header.innerHTML = `
      <h2 class="research-project-title">${escHtml(project.name)}</h2>
      <div class="research-project-actions-header">
        <button class="btn btn-ghost btn-sm" id="research-open-folder" title="Open in Finder">
          <span class="ms ms-sm">folder_open</span>
        </button>
        <button class="btn btn-ghost btn-sm btn-error" id="research-delete-project">Delete</button>
      </div>
    `;

    $('research-open-folder')?.addEventListener('click', () => {
      const ap = _state.getActiveProject();
      if (ap) workspace.openInFinder(ap.id);
    });

    $('research-delete-project')?.addEventListener('click', deleteCurrentProject);
  }

  renderFindings();
  renderSourcesPanel();
  renderProjectList();
}

// ── Findings ───────────────────────────────────────────────────────────────

export function renderFindings() {
  const container = $('research-findings-grid');
  if (!container) return;

  const findings = _state.getFindings();

  if (!findings.length) {
    container.innerHTML = `
      <div class="research-findings-empty">
        <p>No findings yet. Enter a research query above to get started.</p>
      </div>
    `;
    return;
  }

  container.innerHTML = findings
    .map(
      (finding) => `
    <div class="research-finding-card" data-id="${finding.id}">
      <div class="research-finding-header">
        <div class="research-finding-query">${escHtml(finding.query)}</div>
        <div class="research-finding-date">${new Date(finding.created).toLocaleDateString()}</div>
      </div>

      ${finding.summary ? `<div class="research-finding-summary">${escHtml(finding.summary)}</div>` : ''}

      ${
        finding.keyPoints.length
          ? `
        <div class="research-finding-keypoints">
          ${finding.keyPoints
            .slice(0, 3)
            .map(
              (point) => `
            <div class="research-keypoint">
              <span class="keypoint-icon"><span class="ms ms-sm">lightbulb</span></span>
              <span class="keypoint-text">${escHtml(point)}</span>
            </div>
          `,
            )
            .join('')}
        </div>
      `
          : ''
      }

      <div class="research-finding-sources">
        ${finding.sources
          .slice(0, 3)
          .map(
            (s) => `
          <a href="${s.url}" target="_blank" class="research-source-chip" title="${escHtml(s.title)}">
            ${escHtml(extractDomain(s.url))}
            <span class="source-credibility">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `,
          )
          .join('')}
        ${finding.sources.length > 3 ? `<span class="research-source-more">+${finding.sources.length - 3} more</span>` : ''}
      </div>

      <div class="research-finding-actions">
        <button class="btn btn-ghost btn-xs research-action-dig" data-id="${finding.id}" title="Research this deeper">
          <span class="ms ms-sm">search</span> Dig Deeper
        </button>
        <button class="btn btn-ghost btn-xs research-action-related" data-id="${finding.id}" title="Find related topics">
          <span class="ms ms-sm">link</span> Related
        </button>
        <button class="btn btn-ghost btn-xs research-action-expand" data-id="${finding.id}" title="View full content">
          <span class="ms ms-sm">description</span> Full
        </button>
        <button class="btn btn-ghost btn-xs btn-error research-action-delete" data-id="${finding.id}" title="Delete">
          ✕
        </button>
      </div>
    </div>
  `,
    )
    .join('');

  // Wire up action buttons
  container.querySelectorAll('.research-action-dig').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = findings.find((f) => f.id === id);
      if (finding) {
        const input = $('research-topic-input') as HTMLInputElement;
        if (input) {
          input.value = `Dig deeper into: ${finding.query}. Focus on specifics, edge cases, and detailed examples.`;
          input.focus();
        }
      }
    });
  });

  container.querySelectorAll('.research-action-related').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = findings.find((f) => f.id === id);
      if (finding) {
        const input = $('research-topic-input') as HTMLInputElement;
        if (input) {
          input.value = `Find topics related to: ${finding.query}. What are adjacent concepts, alternatives, or complementary approaches?`;
          input.focus();
        }
      }
    });
  });

  container.querySelectorAll('.research-action-expand').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = findings.find((f) => f.id === id);
      if (finding) showFindingDetail(finding);
    });
  });

  container.querySelectorAll('.research-action-delete').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const activeProject = _state.getActiveProject();
      if (id && activeProject && (await confirmModal('Delete this finding?'))) {
        await workspace.deleteFinding(activeProject.id, id);
        _state.setFindings(await workspace.listFindings(activeProject.id));
        renderFindings();
        renderSourcesPanel();
      }
    });
  });
}

// ── Sources panel ──────────────────────────────────────────────────────────

export function renderSourcesPanel() {
  const panel = $('research-sources-panel');
  const activeProject = _state.getActiveProject();
  if (!panel || !activeProject) return;

  workspace.getAllSources(activeProject.id).then((sources) => {
    if (!sources.length) {
      panel.innerHTML =
        '<div class="research-sources-empty">Sources will appear here as you research</div>';
      return;
    }

    panel.innerHTML = `
      <div class="research-sources-header">
        <span>${sources.length} sources</span>
      </div>
      <div class="research-sources-list">
        ${sources
          .slice(0, 10)
          .map(
            (s) => `
          <a href="${s.url}" target="_blank" class="research-source-item">
            <span class="research-source-domain">${escHtml(extractDomain(s.url))}</span>
            <span class="research-source-cred">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `,
          )
          .join('')}
      </div>
    `;
  });
}

// ── Finding detail modal ───────────────────────────────────────────────────

export function showFindingDetail(finding: ResearchFinding) {
  const modal = $('research-detail-modal');
  const content = $('research-detail-content');
  if (!modal || !content) return;

  content.innerHTML = `
    <div class="research-detail-header">
      <h2>${escHtml(finding.query)}</h2>
      <span class="research-detail-date">${new Date(finding.created).toLocaleString()}</span>
    </div>

    ${
      finding.keyPoints.length
        ? `
      <div class="research-detail-section">
        <h3>Key Points</h3>
        <ul>
          ${finding.keyPoints.map((p) => `<li>${escHtml(p)}</li>`).join('')}
        </ul>
      </div>
    `
        : ''
    }

    <div class="research-detail-section">
      <h3>Full Content</h3>
      <div class="research-detail-body">${formatMarkdown(finding.content)}</div>
    </div>

    <div class="research-detail-section">
      <h3>Sources (${finding.sources.length})</h3>
      <div class="research-detail-sources">
        ${finding.sources
          .map(
            (s) => `
          <a href="${s.url}" target="_blank" class="research-detail-source">
            <span class="source-title">${escHtml(s.title)}</span>
            <span class="source-url">${escHtml(s.url)}</span>
            <span class="source-cred">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `,
          )
          .join('')}
      </div>
    </div>
  `;

  modal.style.display = 'flex';
}

// ── Research execution ─────────────────────────────────────────────────────

export async function runResearch() {
  const activeProject = _state.getActiveProject();
  if (!activeProject || !isConnected() || _state.getIsResearching()) return;

  const input = $('research-topic-input') as HTMLInputElement;
  const query = input?.value.trim();
  if (!query) return;

  // Reset state
  _state.setIsResearching(true);
  _state.resetLiveState();
  _state.setRunId(null);

  // Show live panel
  const livePanel = $('research-live-panel');
  const findingsArea = $('research-findings-area');
  if (livePanel) livePanel.style.display = '';
  if (findingsArea) findingsArea.classList.add('researching');

  // Clear and show loading state
  const liveContent = $('research-live-content');
  const sourceFeed = $('research-source-feed');
  const progressSteps = $('research-progress-steps');
  if (liveContent) liveContent.innerHTML = '';
  if (sourceFeed) sourceFeed.innerHTML = '';
  if (progressSteps) progressSteps.innerHTML = '';

  // Update button
  const runBtn = $('research-run-btn');
  const stopBtn = $('research-stop-btn');
  if (runBtn) runBtn.style.display = 'none';
  if (stopBtn) stopBtn.style.display = '';

  // Add initial step
  _state.pushLiveStep('Starting research...');
  renderProgressSteps();

  const mode = _state.getResearchMode();
  const sessionKey = `paw-research-${activeProject.id}`;
  const prompt = buildResearchPrompt(query, mode);

  const done = new Promise<string>((resolve) => {
    _state.setStreamResolve(resolve);
    setTimeout(
      () => resolve(_state.getStreamContent() || '(Research timed out)'),
      modeTimeout(mode),
    );
  });

  try {
    const result = await pawEngine.chatSend(sessionKey, prompt);
    if (result.run_id) _state.setRunId(result.run_id);

    const finalText = await done;

    // Parse and save finding
    const now = new Date().toISOString();
    const parsed = workspace.parseAgentResponse(query, finalText, _state.getLiveSources());
    const finding: ResearchFinding = {
      id: workspace.generateFindingId(),
      ...parsed,
      created: now,
      updated: now,
    };

    await workspace.saveFinding(activeProject.id, finding);
    _state.setFindings(await workspace.listFindings(activeProject.id));

    // Clear input
    if (input) input.value = '';
    showToast('Research complete! Finding saved.', 'success');
  } catch (e) {
    console.error('[research] Error:', e);
    showToast(`Research failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    _state.setIsResearching(false);
    _state.setRunId(null);
    _state.setStreamResolve(null);

    // Hide live panel, show findings
    if (livePanel) livePanel.style.display = 'none';
    if (findingsArea) findingsArea.classList.remove('researching');
    if (runBtn) runBtn.style.display = '';
    if (stopBtn) stopBtn.style.display = 'none';

    renderFindings();
    renderSourcesPanel();
  }
}

export async function stopResearch() {
  const activeProject = _state.getActiveProject();
  if (!activeProject) return;

  try {
    await pawEngine.chatAbort(`paw-research-${activeProject.id}`);
  } catch (e) {
    console.warn('[research] Abort error:', e);
  }

  const resolve = _state.getStreamResolve();
  if (resolve) {
    resolve(_state.getStreamContent() || '(Aborted)');
    _state.setStreamResolve(null);
  }
}

export async function generateReport() {
  const activeProject = _state.getActiveProject();
  const findings = _state.getFindings();
  if (!activeProject || !findings.length || !isConnected()) {
    showToast('No findings to generate report from', 'error');
    return;
  }

  const reportModal = $('research-report-modal');
  const reportContent = $('research-report-content');
  if (!reportModal || !reportContent) return;

  reportModal.style.display = 'flex';
  reportContent.innerHTML =
    '<div class="loading-dots"><span></span><span></span><span></span></div><p>Generating report...</p>';

  const findingsText = findings
    .map(
      (f, i) =>
        `## Finding ${i + 1}: ${f.query}\n\n${f.summary || ''}\n\n${f.content}\n\nSources: ${f.sources.map((s) => s.url).join(', ')}`,
    )
    .join('\n\n---\n\n');

  const sessionKey = `paw-research-${activeProject.id}`;

  _state.setIsResearching(true);
  _state.setStreamContent('');

  const done = new Promise<string>((resolve) => {
    _state.setStreamResolve(resolve);
    setTimeout(
      () => resolve(_state.getStreamContent() || '(Report generation timed out)'),
      180_000,
    );
  });

  try {
    await pawEngine.chatSend(
      sessionKey,
      `Based on all the research findings below, write a comprehensive, well-structured report. Include:\n\n1. Executive Summary (2-3 paragraphs)\n2. Key Findings (organized by theme)\n3. Detailed Analysis\n4. Conclusions and Recommendations\n5. Sources Bibliography\n\nUse markdown formatting.\n\n${findingsText}`,
    );

    const reportText = await done;

    // Save report
    const report: workspace.ResearchReport = {
      id: workspace.generateFindingId(),
      title: `Research Report — ${new Date().toLocaleDateString()}`,
      created: new Date().toISOString(),
      content: reportText,
      findingIds: findings.map((f) => f.id),
    };

    await workspace.saveReport(activeProject.id, report);

    reportContent.innerHTML = formatMarkdown(reportText);
    showToast('Report generated and saved!', 'success');
  } catch (e) {
    reportContent.innerHTML = `<p class="error">Failed to generate report: ${e instanceof Error ? e.message : e}</p>`;
  } finally {
    _state.setIsResearching(false);
    _state.setStreamResolve(null);
  }
}

export async function createNewProject() {
  const name = await _state.getPromptModal()?.('Research project name:', 'My Research');
  if (!name) return;

  try {
    const project = await workspace.createResearchProject(name);
    await openProject(project.id);
    showToast('Project created!', 'success');
  } catch (e) {
    showToast(`Failed to create project: ${e}`, 'error');
  }
}

export async function deleteCurrentProject() {
  const activeProject = _state.getActiveProject();
  if (!activeProject) return;
  if (
    !(await confirmModal(
      `Delete "${activeProject.name}" and all its findings? This cannot be undone.`,
    ))
  )
    return;

  try {
    await workspace.deleteResearchProject(activeProject.id);
    (
      _state as unknown as { setActiveProject: (p: workspace.ResearchProject | null) => void }
    ).setActiveProject(null);
    _state.setFindings([]);

    const empty = $('research-empty');
    const main = $('research-workspace');
    if (empty) empty.style.display = 'flex';
    if (main) main.style.display = 'none';

    await _state.reloadProjects();
    showToast('Project deleted', 'success');
  } catch (e) {
    showToast(`Failed to delete: ${e}`, 'error');
  }
}
