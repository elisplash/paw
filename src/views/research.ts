// Research View — Redesigned with live sources, structured findings, iteration
// Saves to ~/Documents/Paw/Research as markdown files

import { pawEngine } from '../engine';
import * as workspace from '../workspace';
import type { ResearchProject, ResearchFinding, ResearchSource, ResearchReport } from '../workspace';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let wsConnected = false;
let _activeProject: ResearchProject | null = null;
let _findings: ResearchFinding[] = [];
let _isResearching = false;
let _researchMode: 'quick' | 'deep' = 'quick';
let _runId: string | null = null;
let _streamContent = '';
let _streamResolve: ((text: string) => void) | null = null;
let _liveSources: ResearchSource[] = [];
let _liveSteps: string[] = [];

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

// For agent event routing
export function isStreaming(): boolean {
  return _isResearching;
}

export function getRunId(): string | null {
  return _runId;
}

// ── Live streaming handlers ────────────────────────────────────────────────

export function appendDelta(text: string) {
  _streamContent += text;
  
  // Parse for URLs being fetched (agent typically outputs these)
  const urlMatches = text.match(/https?:\/\/[^\s\])"']+/g);
  if (urlMatches) {
    for (const url of urlMatches) {
      if (!_liveSources.some(s => s.url === url)) {
        const source: ResearchSource = {
          url,
          title: extractDomain(url),
          credibility: 3,
          extractedAt: new Date().toISOString(),
          snippets: [],
        };
        _liveSources.push(source);
        renderLiveSourceFeed();
      }
    }
  }
  
  // Update live content display
  const liveContent = $('research-live-content');
  if (liveContent) {
    liveContent.textContent = _streamContent.slice(-2000); // Show last 2000 chars
    liveContent.scrollTop = liveContent.scrollHeight;
  }
  
  // Parse for progress indicators
  parseProgressStep(text);
}

export function resolveStream(text?: string) {
  if (_streamResolve) {
    _streamResolve(text ?? _streamContent);
    _streamResolve = null;
  }
}

export function getContent(): string {
  return _streamContent;
}

export function setContent(text: string) {
  _streamContent = text;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function extractDomain(url: string): string {
  try {
    return new URL(url).hostname.replace('www.', '');
  } catch {
    return url.slice(0, 30);
  }
}

function showToast(message: string, type: 'info' | 'success' | 'error' = 'info') {
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
  }, 3500);
}

function parseProgressStep(text: string) {
  // Detect common research patterns
  const patterns = [
    { regex: /searching|search for/i, step: 'Searching the web...' },
    { regex: /reading|fetching|loading/i, step: 'Reading sources...' },
    { regex: /analyzing|analysis/i, step: 'Analyzing content...' },
    { regex: /found|discovered/i, step: 'Found relevant information' },
    { regex: /summarizing|summary/i, step: 'Summarizing findings...' },
  ];
  
  for (const { regex, step } of patterns) {
    if (regex.test(text) && !_liveSteps.includes(step)) {
      _liveSteps.push(step);
      renderProgressSteps();
      break;
    }
  }
}

// ── Callbacks ──────────────────────────────────────────────────────────────

let promptModalFn: ((title: string, placeholder?: string) => Promise<string | null>) | null = null;

export function configure(opts: {
  promptModal: (title: string, placeholder?: string) => Promise<string | null>;
}) {
  promptModalFn = opts.promptModal;
}

// ── Render Functions ───────────────────────────────────────────────────────

function renderLiveSourceFeed() {
  const feed = $('research-source-feed');
  if (!feed) return;
  
  feed.innerHTML = _liveSources.slice(-8).map(source => `
    <div class="research-live-source">
      <span class="research-live-source-icon">🌐</span>
      <span class="research-live-source-domain">${escHtml(source.title)}</span>
    </div>
  `).join('');
}

function renderProgressSteps() {
  const container = $('research-progress-steps');
  if (!container) return;
  
  container.innerHTML = _liveSteps.map((step, i) => `
    <div class="research-progress-step ${i === _liveSteps.length - 1 ? 'active' : 'done'}">
      <span class="research-step-icon">${i === _liveSteps.length - 1 ? '◉' : '✓'}</span>
      <span class="research-step-text">${escHtml(step)}</span>
    </div>
  `).join('');
}

export async function loadProjects() {
  await workspace.ensureWorkspace();
  
  const list = $('research-project-list');
  const empty = $('research-empty');
  const main = $('research-workspace');
  if (!list) return;
  
  const projects = await workspace.listResearchProjects();
  list.innerHTML = '';
  
  if (!projects.length && !_activeProject) {
    if (empty) empty.style.display = 'flex';
    if (main) main.style.display = 'none';
    return;
  }
  
  for (const p of projects) {
    const item = document.createElement('div');
    item.className = `research-project-item${p.id === _activeProject?.id ? ' active' : ''}`;
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
    .flatMap(p => p.queries.slice(-3).map(q => ({ query: q, projectId: p.id, projectName: p.name })))
    .slice(0, 5);
  
  const recentList = $('research-recent-queries');
  if (recentList && recentQueries.length) {
    recentList.innerHTML = recentQueries.map(r => `
      <div class="research-recent-query" data-project="${r.projectId}" data-query="${escHtml(r.query)}">
        <span class="research-recent-icon">↩</span>
        <span class="research-recent-text">${escHtml(r.query.slice(0, 40))}${r.query.length > 40 ? '...' : ''}</span>
      </div>
    `).join('');
    
    recentList.querySelectorAll('.research-recent-query').forEach(el => {
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

async function openProject(id: string) {
  const project = await workspace.getResearchProject(id);
  if (!project) return;
  
  _activeProject = project;
  _findings = await workspace.listFindings(id);
  
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
          <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </button>
        <button class="btn btn-ghost btn-sm btn-error" id="research-delete-project">Delete</button>
      </div>
    `;
    
    $('research-open-folder')?.addEventListener('click', () => {
      if (_activeProject) workspace.openInFinder(_activeProject.id);
    });
    
    $('research-delete-project')?.addEventListener('click', deleteCurrentProject);
  }
  
  renderFindings();
  renderSourcesPanel();
  loadProjects();
}

function renderFindings() {
  const container = $('research-findings-grid');
  if (!container) return;
  
  if (!_findings.length) {
    container.innerHTML = `
      <div class="research-findings-empty">
        <p>No findings yet. Enter a research query above to get started.</p>
      </div>
    `;
    return;
  }
  
  container.innerHTML = _findings.map(finding => `
    <div class="research-finding-card" data-id="${finding.id}">
      <div class="research-finding-header">
        <div class="research-finding-query">${escHtml(finding.query)}</div>
        <div class="research-finding-date">${new Date(finding.created).toLocaleDateString()}</div>
      </div>
      
      ${finding.summary ? `<div class="research-finding-summary">${escHtml(finding.summary)}</div>` : ''}
      
      ${finding.keyPoints.length ? `
        <div class="research-finding-keypoints">
          ${finding.keyPoints.slice(0, 3).map(point => `
            <div class="research-keypoint">
              <span class="keypoint-icon">💡</span>
              <span class="keypoint-text">${escHtml(point)}</span>
            </div>
          `).join('')}
        </div>
      ` : ''}
      
      <div class="research-finding-sources">
        ${finding.sources.slice(0, 3).map(s => `
          <a href="${s.url}" target="_blank" class="research-source-chip" title="${escHtml(s.title)}">
            ${escHtml(extractDomain(s.url))}
            <span class="source-credibility">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `).join('')}
        ${finding.sources.length > 3 ? `<span class="research-source-more">+${finding.sources.length - 3} more</span>` : ''}
      </div>
      
      <div class="research-finding-actions">
        <button class="btn btn-ghost btn-xs research-action-dig" data-id="${finding.id}" title="Research this deeper">
          🔍 Dig Deeper
        </button>
        <button class="btn btn-ghost btn-xs research-action-related" data-id="${finding.id}" title="Find related topics">
          🔗 Related
        </button>
        <button class="btn btn-ghost btn-xs research-action-expand" data-id="${finding.id}" title="View full content">
          📄 Full
        </button>
        <button class="btn btn-ghost btn-xs btn-error research-action-delete" data-id="${finding.id}" title="Delete">
          ✕
        </button>
      </div>
    </div>
  `).join('');
  
  // Wire up action buttons
  container.querySelectorAll('.research-action-dig').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = _findings.find(f => f.id === id);
      if (finding) {
        const input = $('research-topic-input') as HTMLInputElement;
        if (input) {
          input.value = `Dig deeper into: ${finding.query}. Focus on specifics, edge cases, and detailed examples.`;
          input.focus();
        }
      }
    });
  });
  
  container.querySelectorAll('.research-action-related').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = _findings.find(f => f.id === id);
      if (finding) {
        const input = $('research-topic-input') as HTMLInputElement;
        if (input) {
          input.value = `Find topics related to: ${finding.query}. What are adjacent concepts, alternatives, or complementary approaches?`;
          input.focus();
        }
      }
    });
  });
  
  container.querySelectorAll('.research-action-expand').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      const finding = _findings.find(f => f.id === id);
      if (finding) showFindingDetail(finding);
    });
  });
  
  container.querySelectorAll('.research-action-delete').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const id = btn.getAttribute('data-id');
      if (id && _activeProject && confirm('Delete this finding?')) {
        await workspace.deleteFinding(_activeProject.id, id);
        _findings = await workspace.listFindings(_activeProject.id);
        renderFindings();
        renderSourcesPanel();
      }
    });
  });
}

function renderSourcesPanel() {
  const panel = $('research-sources-panel');
  if (!panel || !_activeProject) return;
  
  workspace.getAllSources(_activeProject.id).then(sources => {
    if (!sources.length) {
      panel.innerHTML = '<div class="research-sources-empty">Sources will appear here as you research</div>';
      return;
    }
    
    panel.innerHTML = `
      <div class="research-sources-header">
        <span>${sources.length} sources</span>
      </div>
      <div class="research-sources-list">
        ${sources.slice(0, 10).map(s => `
          <a href="${s.url}" target="_blank" class="research-source-item">
            <span class="research-source-domain">${escHtml(extractDomain(s.url))}</span>
            <span class="research-source-cred">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `).join('')}
      </div>
    `;
  });
}

function showFindingDetail(finding: ResearchFinding) {
  const modal = $('research-detail-modal');
  const content = $('research-detail-content');
  if (!modal || !content) return;
  
  content.innerHTML = `
    <div class="research-detail-header">
      <h2>${escHtml(finding.query)}</h2>
      <span class="research-detail-date">${new Date(finding.created).toLocaleString()}</span>
    </div>
    
    ${finding.keyPoints.length ? `
      <div class="research-detail-section">
        <h3>Key Points</h3>
        <ul>
          ${finding.keyPoints.map(p => `<li>${escHtml(p)}</li>`).join('')}
        </ul>
      </div>
    ` : ''}
    
    <div class="research-detail-section">
      <h3>Full Content</h3>
      <div class="research-detail-body">${formatMarkdown(finding.content)}</div>
    </div>
    
    <div class="research-detail-section">
      <h3>Sources (${finding.sources.length})</h3>
      <div class="research-detail-sources">
        ${finding.sources.map(s => `
          <a href="${s.url}" target="_blank" class="research-detail-source">
            <span class="source-title">${escHtml(s.title)}</span>
            <span class="source-url">${escHtml(s.url)}</span>
            <span class="source-cred">${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}</span>
          </a>
        `).join('')}
      </div>
    </div>
  `;
  
  modal.style.display = 'flex';
}

function formatMarkdown(text: string): string {
  return escHtml(text)
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.*?)\*/g, '<em>$1</em>')
    .replace(/```[\s\S]*?```/g, m => `<pre><code>${m.slice(3, -3)}</code></pre>`)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/^[-•] (.+)$/gm, '<li>$1</li>')
    .replace(/\n/g, '<br>');
}

// ── Research Execution ─────────────────────────────────────────────────────

async function runResearch() {
  if (!_activeProject || !wsConnected || _isResearching) return;
  
  const input = $('research-topic-input') as HTMLInputElement;
  const query = input?.value.trim();
  if (!query) return;
  
  // Reset state
  _isResearching = true;
  _streamContent = '';
  _liveSources = [];
  _liveSteps = [];
  _runId = null;
  
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
  _liveSteps.push('Starting research...');
  renderProgressSteps();
  
  const sessionKey = `paw-research-${_activeProject.id}`;
  const prompt = _researchMode === 'deep'
    ? `Research this topic thoroughly and comprehensively. Browse the web, find at least 10 diverse sources, cross-reference information, and provide detailed findings with specific data points, examples, and source URLs. Be exhaustive.\n\nTopic: ${query}`
    : `Research this topic efficiently. Find 3-5 reliable sources, extract key information, and provide a focused summary with the most important findings and source URLs.\n\nTopic: ${query}`;
  
  const done = new Promise<string>((resolve) => {
    _streamResolve = resolve;
    setTimeout(() => resolve(_streamContent || '(Research timed out)'), _researchMode === 'deep' ? 300_000 : 120_000);
  });
  
  try {
    const result = await pawEngine.chatSend(sessionKey, prompt);
    if (result.run_id) _runId = result.run_id;
    
    const finalText = await done;
    
    // Parse and save finding
    const now = new Date().toISOString();
    const parsed = workspace.parseAgentResponse(query, finalText, _liveSources);
    const finding: ResearchFinding = {
      id: workspace.generateFindingId(),
      ...parsed,
      created: now,
      updated: now,
    };
    
    await workspace.saveFinding(_activeProject.id, finding);
    _findings = await workspace.listFindings(_activeProject.id);
    
    // Clear input
    if (input) input.value = '';
    showToast('Research complete! Finding saved.', 'success');
    
  } catch (e) {
    console.error('[research] Error:', e);
    showToast(`Research failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    _isResearching = false;
    _runId = null;
    _streamResolve = null;
    
    // Hide live panel, show findings
    if (livePanel) livePanel.style.display = 'none';
    if (findingsArea) findingsArea.classList.remove('researching');
    if (runBtn) runBtn.style.display = '';
    if (stopBtn) stopBtn.style.display = 'none';
    
    renderFindings();
    renderSourcesPanel();
  }
}

async function stopResearch() {
  if (!_activeProject) return;
  
  try {
    await pawEngine.chatAbort(`paw-research-${_activeProject.id}`);
  } catch (e) {
    console.warn('[research] Abort error:', e);
  }
  
  if (_streamResolve) {
    _streamResolve(_streamContent || '(Aborted)');
    _streamResolve = null;
  }
}

async function generateReport() {
  if (!_activeProject || !_findings.length || !wsConnected) {
    showToast('No findings to generate report from', 'error');
    return;
  }
  
  const reportModal = $('research-report-modal');
  const reportContent = $('research-report-content');
  if (!reportModal || !reportContent) return;
  
  reportModal.style.display = 'flex';
  reportContent.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div><p>Generating report...</p>';
  
  const findingsText = _findings.map((f, i) => 
    `## Finding ${i + 1}: ${f.query}\n\n${f.summary || ''}\n\n${f.content}\n\nSources: ${f.sources.map(s => s.url).join(', ')}`
  ).join('\n\n---\n\n');
  
  const sessionKey = `paw-research-${_activeProject.id}`;
  
  _isResearching = true;
  _streamContent = '';
  
  const done = new Promise<string>((resolve) => {
    _streamResolve = resolve;
    setTimeout(() => resolve(_streamContent || '(Report generation timed out)'), 180_000);
  });
  
  try {
    await pawEngine.chatSend(sessionKey,
      `Based on all the research findings below, write a comprehensive, well-structured report. Include:\n\n1. Executive Summary (2-3 paragraphs)\n2. Key Findings (organized by theme)\n3. Detailed Analysis\n4. Conclusions and Recommendations\n5. Sources Bibliography\n\nUse markdown formatting.\n\n${findingsText}`
    );
    
    const reportText = await done;
    
    // Save report
    const report: ResearchReport = {
      id: workspace.generateFindingId(),
      title: `Research Report — ${new Date().toLocaleDateString()}`,
      created: new Date().toISOString(),
      content: reportText,
      findingIds: _findings.map(f => f.id),
    };
    
    await workspace.saveReport(_activeProject.id, report);
    
    reportContent.innerHTML = formatMarkdown(reportText);
    showToast('Report generated and saved!', 'success');
    
  } catch (e) {
    reportContent.innerHTML = `<p class="error">Failed to generate report: ${e instanceof Error ? e.message : e}</p>`;
  } finally {
    _isResearching = false;
    _streamResolve = null;
  }
}

async function createNewProject() {
  const name = await promptModalFn?.('Research project name:', 'My Research');
  if (!name) return;
  
  try {
    const project = await workspace.createResearchProject(name);
    await openProject(project.id);
    showToast('Project created!', 'success');
  } catch (e) {
    showToast(`Failed to create project: ${e}`, 'error');
  }
}

async function deleteCurrentProject() {
  if (!_activeProject) return;
  if (!confirm(`Delete "${_activeProject.name}" and all its findings? This cannot be undone.`)) return;
  
  try {
    await workspace.deleteResearchProject(_activeProject.id);
    _activeProject = null;
    _findings = [];
    
    const empty = $('research-empty');
    const main = $('research-workspace');
    if (empty) empty.style.display = 'flex';
    if (main) main.style.display = 'none';
    
    await loadProjects();
    showToast('Project deleted', 'success');
  } catch (e) {
    showToast(`Failed to delete: ${e}`, 'error');
  }
}

// ── Event Wiring ───────────────────────────────────────────────────────────

export function initResearchEvents() {
  // New project buttons
  $('research-new-project')?.addEventListener('click', createNewProject);
  $('research-create-first')?.addEventListener('click', createNewProject);
  
  // Research mode toggle
  $('research-mode-quick')?.addEventListener('click', () => {
    _researchMode = 'quick';
    $('research-mode-quick')?.classList.add('active');
    $('research-mode-deep')?.classList.remove('active');
  });
  
  $('research-mode-deep')?.addEventListener('click', () => {
    _researchMode = 'deep';
    $('research-mode-deep')?.classList.add('active');
    $('research-mode-quick')?.classList.remove('active');
  });
  
  // Run research
  $('research-run-btn')?.addEventListener('click', runResearch);
  $('research-topic-input')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      runResearch();
    }
  });
  
  // Stop research
  $('research-stop-btn')?.addEventListener('click', stopResearch);
  
  // Generate report
  $('research-generate-report')?.addEventListener('click', generateReport);
  
  // Close modals
  $('research-detail-close')?.addEventListener('click', () => {
    const modal = $('research-detail-modal');
    if (modal) modal.style.display = 'none';
  });
  
  $('research-report-close')?.addEventListener('click', () => {
    const modal = $('research-report-modal');
    if (modal) modal.style.display = 'none';
  });
  
  // Open workspace folder
  $('research-open-workspace')?.addEventListener('click', () => {
    workspace.openInFinder();
  });
}

// Re-export for backwards compatibility
export { loadProjects as loadResearchProjects };
