// Research View — Index (orchestration, state, exports)
// Saves to ~/Documents/Paw/Research as markdown files

import type { ResearchProject, ResearchFinding, ResearchSource } from '../../workspace';
import * as workspace from '../../workspace';
import { $ } from '../../components/helpers';
import { extractDomain, parseProgressStep, type ResearchMode } from './atoms';
import {
  setMoleculesState,
  renderProjectList,
  renderLiveSourceFeed,
  renderProgressSteps,
  runResearch,
  stopResearch,
  generateReport,
  createNewProject,
} from './molecules';

// ── Module state ───────────────────────────────────────────────────────────

let _activeProject: ResearchProject | null = null;
let _findings: ResearchFinding[] = [];
let _isResearching = false;
let _researchMode: ResearchMode = 'quick';
let _runId: string | null = null;
let _streamContent = '';
let _streamResolve: ((text: string) => void) | null = null;
let _liveSources: ResearchSource[] = [];
let _liveSteps: string[] = [];

let promptModalFn: ((title: string, placeholder?: string) => Promise<string | null>) | null = null;

// ── State bridge for molecules ─────────────────────────────────────────────

function initMoleculesState() {
  setMoleculesState({
    getActiveProject: () => _activeProject,
    setActiveProject: (p: ResearchProject | null) => {
      _activeProject = p;
    },
    getFindings: () => _findings,
    setFindings: (f: ResearchFinding[]) => {
      _findings = f;
    },
    getIsResearching: () => _isResearching,
    setIsResearching: (v: boolean) => {
      _isResearching = v;
    },
    getResearchMode: () => _researchMode,
    getStreamContent: () => _streamContent,
    setStreamContent: (s: string) => {
      _streamContent = s;
    },
    getStreamResolve: () => _streamResolve,
    setStreamResolve: (fn: ((text: string) => void) | null) => {
      _streamResolve = fn;
    },
    getLiveSources: () => _liveSources,
    pushLiveSource: (s: ResearchSource) => {
      _liveSources.push(s);
    },
    getLiveSteps: () => _liveSteps,
    pushLiveStep: (s: string) => {
      _liveSteps.push(s);
    },
    setRunId: (id: string | null) => {
      _runId = id;
    },
    resetLiveState: () => {
      _streamContent = '';
      _liveSources = [];
      _liveSteps = [];
    },
    getPromptModal: () => promptModalFn,
    reloadProjects: () => loadProjects(),
  } as ReturnType<typeof setMoleculesState> extends void
    ? Parameters<typeof setMoleculesState>[0] & {
        setActiveProject: (p: ResearchProject | null) => void;
      }
    : never);
}

// ── Public API for agent event routing ─────────────────────────────────────

export function isStreaming(): boolean {
  return _isResearching;
}

export function getRunId(): string | null {
  return _runId;
}

// ── Live streaming handlers ────────────────────────────────────────────────

export function appendDelta(text: string) {
  _streamContent += text;

  // Parse for URLs being fetched
  const urlMatches = text.match(/https?:\/\/[^\s\])"']+/g);
  if (urlMatches) {
    for (const url of urlMatches) {
      if (!_liveSources.some((s) => s.url === url)) {
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
    liveContent.textContent = _streamContent.slice(-2000);
    liveContent.scrollTop = liveContent.scrollHeight;
  }

  // Parse for progress indicators
  const newStep = parseProgressStep(text, _liveSteps);
  if (newStep) {
    _liveSteps.push(newStep);
    renderProgressSteps();
  }
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

// ── Callbacks ──────────────────────────────────────────────────────────────

export function configure(opts: {
  promptModal: (title: string, placeholder?: string) => Promise<string | null>;
}) {
  promptModalFn = opts.promptModal;
}

// ── Load & init ────────────────────────────────────────────────────────────

export async function loadProjects() {
  initMoleculesState();
  await renderProjectList();
}

export function initResearchEvents() {
  initMoleculesState();

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
