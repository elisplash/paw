// Paw — Application Entry Point
import { isEngineMode, setEngineMode, startEngineBridge } from './engine-bridge';
import { pawEngine } from './engine';
import { initDb, initDbEncryption } from './db';
import { appState } from './state/index';
import { escHtml, populateModelSelect, promptModal, icon } from './components/helpers';
import { showToast } from './components/toast';
import { initTheme } from './components/molecules/theme';
import { initHILModal } from './components/molecules/hil_modal';
import { initChatListeners, switchToAgent, populateAgentSelect, appendStreamingDelta, recordTokenUsage, updateContextLimitFromModel } from './engine/organisms/chat_controller';
import { registerStreamHandlers, registerResearchRouter } from './engine/molecules/event_bus';
import * as ResearchModule from './views/research';

// ── Wire event_bus callbacks (engine ← view layer) ──
registerStreamHandlers({
  onDelta: appendStreamingDelta,
  onToken: recordTokenUsage,
  onModel: updateContextLimitFromModel,
});
registerResearchRouter({
  isStreaming: ResearchModule.isStreaming,
  getRunId: ResearchModule.getRunId,
  appendDelta: ResearchModule.appendDelta,
  resolveStream: ResearchModule.resolveStream,
});
import { initChannels, openMemoryFile, autoStartConfiguredChannels, closeChannelSetup } from './views/channels';
import { initContent } from './views/content';
import { switchView, showView } from './views/router';
import { initSettingsTabs } from './views/settings-tabs';
import * as SettingsModule from './views/settings';
import * as ModelsSettings from './views/settings-models';
import * as AgentDefaultsSettings from './views/settings-agent-defaults';
import * as SessionsSettings from './views/settings-sessions';
import * as VoiceSettings from './views/settings-voice';
import * as SkillsSettings from './views/settings-skills';
import { setConnected as setSettingsConnected } from './views/settings-config';
import * as AutomationsModule from './views/automations';
import * as MemoryPalaceModule from './views/memory-palace';
import * as MailModule from './views/mail';
import * as SkillsModule from './views/skills';
import * as FoundryModule from './views/foundry';
import * as NodesModule from './views/nodes';
import * as ProjectsModule from './views/projects';
import * as AgentsModule from './views/agents';
import * as TasksModule from './views/tasks';
import * as OrchestratorModule from './views/orchestrator';
import * as TradingModule from './views/trading';

// ── Tauri bridge ─────────────────────────────────────────────────────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
    event: { listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void> };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const listen = tauriWindow.__TAURI__?.event?.listen;

// ── Global error handlers ──────────────────────────────────────────────────────
function crashLog(msg: string) {
  try {
    const log = JSON.parse(localStorage.getItem('paw-crash-log') || '[]') as string[];
    log.push(`${new Date().toISOString()} ${msg}`);
    while (log.length > 50) log.shift();
    localStorage.setItem('paw-crash-log', JSON.stringify(log));
  } catch { /* localStorage might be full */ }
}
window.addEventListener('unhandledrejection', (event) => {
  const msg = event.reason?.message ?? event.reason ?? 'unknown';
  crashLog(`unhandledrejection: ${msg}`);
  console.error('Unhandled promise rejection:', msg);
  event.preventDefault();
});
window.addEventListener('error', (event) => {
  const msg = event.error?.message ?? event.message ?? 'unknown';
  crashLog(`error: ${msg}`);
  console.error('Uncaught error:', msg);
});

// ── DOM convenience ────────────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);


// ── Model selector ──────────────────────────────────────────────────────────────────
async function refreshModelLabel() {
  const chatModelSelect = $('chat-model-select') as HTMLSelectElement | null;
  if (!chatModelSelect) return;
  try {
    const cfg = await pawEngine.getConfig();
    const defaultModel = cfg.default_model || '';
    const providers = cfg.providers ?? [];
    const currentVal = chatModelSelect.value;
    populateModelSelect(chatModelSelect, providers, {
      defaultLabel: 'Default Model',
      currentValue: currentVal && currentVal !== 'default' ? currentVal : 'default',
      showDefaultModel: defaultModel || undefined,
    });
  } catch { /* leave as-is */ }
}
(window as unknown as Record<string, unknown>).__refreshModelLabel = refreshModelLabel;

// ── Engine connection ───────────────────────────────────────────────────────────
async function connectEngine(): Promise<boolean> {
  if (isEngineMode()) {
    console.log('[main] Engine mode — using Tauri IPC');
    await startEngineBridge();
    appState.wsConnected = true;
    setSettingsConnected(true);
    SettingsModule.setWsConnected(true);
    MemoryPalaceModule.setWsConnected(true);
    MailModule.setWsConnected(true);
    SkillsModule.setWsConnected(true);
    FoundryModule.setWsConnected(true);
    ResearchModule.setWsConnected(true);
    NodesModule.setWsConnected(true);
    AutomationsModule.setWsConnected(true);
    TradingModule.setWsConnected(true);

    const statusDot = $('status-dot');
    const statusText = $('status-text');
    const chatAgentName = $('chat-agent-name');
    const chatAvatarEl = $('chat-avatar');
    statusDot?.classList.add('connected');
    statusDot?.classList.remove('error');
    if (statusText) statusText.textContent = 'Engine';

    const initAgent = AgentsModule.getCurrentAgent();
    if (chatAgentName) {
      chatAgentName.innerHTML = initAgent
        ? `${AgentsModule.spriteAvatar(initAgent.avatar, 20)} ${escHtml(initAgent.name)}`
        : `${AgentsModule.spriteAvatar('5', 20)} Paw`;
    }
    if (chatAvatarEl && initAgent) chatAvatarEl.innerHTML = AgentsModule.spriteAvatar(initAgent.avatar, 32);

    refreshModelLabel();
    TasksModule.startCronTimer();
    if (listen) {
      listen<{ task_id: string; status: string }>('task-updated', (event) => {
        TasksModule.onTaskUpdated(event.payload);
      });
    }

    pawEngine.autoSetup().then(result => {
      if (result.action === 'ollama_added') {
        showToast(result.message || `Ollama detected! Using model '${result.model}'.`, 'success');
        ModelsSettings.loadModelsSettings();
      }
    }).catch(e => console.warn('[main] Auto-setup failed (non-fatal):', e));

    pawEngine.ensureEmbeddingReady().then(status => {
      if (status.error) console.warn('[main] Ollama embedding setup:', status.error);
      else console.log(`[main] Ollama ready: model=${status.model_name} dims=${status.embedding_dims}`);
    }).catch(e => console.warn('[main] Ollama auto-init failed (non-fatal):', e));

    return true;
  }
  console.warn('[main] connectEngine: engine mode should have handled it above');
  return false;
}


// ── Initialize ──────────────────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
  try {
    console.log('[main] Paw starting...');

    for (const el of document.querySelectorAll<HTMLElement>('[data-icon]')) {
      const name = el.dataset.icon;
      if (name) el.innerHTML = icon(name);
    }

    initTheme();

    try {
      const prevLog = localStorage.getItem('paw-crash-log');
      if (prevLog) {
        const entries = JSON.parse(prevLog) as string[];
        if (entries.length) entries.slice(-5).forEach(e => console.warn('  ', e));
      }
    } catch { /* ignore */ }
    crashLog('startup');

    await initDb().catch(e => console.warn('[main] DB init failed:', e));
    await initDbEncryption().catch(e => console.warn('[main] DB encryption init failed:', e));

    MemoryPalaceModule.initPalaceEvents();
    window.addEventListener('palace-open-file', (e: Event) => {
      openMemoryFile((e as CustomEvent).detail as string);
    });

    MailModule.configure({
      switchView,
      setCurrentSession: (key) => { appState.currentSessionKey = key; },
      getChatInput: () => document.getElementById('chat-input') as HTMLTextAreaElement | null,
      closeChannelSetup,
    });
    MailModule.initMailEvents();

    $('refresh-skills-btn')?.addEventListener('click', () => SkillsSettings.loadSkillsSettings());
    FoundryModule.initFoundryEvents();
    ResearchModule.configure({ promptModal });
    ResearchModule.initResearchEvents();

    localStorage.setItem('paw-runtime-mode', 'engine');

    AgentsModule.configure({
      switchView,
      setCurrentAgent: (agentId) => { if (agentId) switchToAgent(agentId); },
    });
    AgentsModule.initAgents();
    AgentsModule.onProfileUpdated((agentId, agent) => {
      const current = AgentsModule.getCurrentAgent();
      const chatAgentName = $('chat-agent-name');
      if (current && current.id === agentId && chatAgentName) {
        chatAgentName.innerHTML = `${AgentsModule.spriteAvatar(agent.avatar, 20)} ${escHtml(agent.name)}`;
      }
      populateAgentSelect();
    });

    NodesModule.initNodesEvents();
    SettingsModule.initSettings();
    initSettingsTabs();
    ModelsSettings.initModelsSettings();
    AgentDefaultsSettings.initAgentDefaultsSettings();
    SessionsSettings.initSessionsSettings();
    VoiceSettings.initVoiceSettings();
    setEngineMode(true);

    ProjectsModule.bindEvents();
    TasksModule.bindTaskEvents();
    OrchestratorModule.initOrchestrator();
    initChannels();
    initContent();
    initChatListeners();
    initHILModal();

    console.log('[main] Pawz engine mode — starting...');
    switchView('dashboard');
    await connectEngine();

    autoStartConfiguredChannels().catch(e => console.warn('[main] Auto-start channels error:', e));
    console.log('[main] Pawz initialized');
  } catch (e) {
    console.error('[main] Init error:', e);
    showView('setup-view');
  }
});
