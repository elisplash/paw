// Paw — Application Entry Point
import { isEngineMode, setEngineMode, startEngineBridge } from './engine-bridge';
import { pawEngine } from './engine';
import { initDb, initDbEncryption, listModelPricing } from './db';
import { initSecuritySettings } from './security';
import { installErrorBoundary, setErrorHandler } from './error-boundary';
import { appState, applyModelPricingOverrides } from './state/index';
import { escHtml, populateModelSelect, promptModal, icon } from './components/helpers';
import { showToast } from './components/toast';
import { initTheme } from './components/molecules/theme';
import { initHILModal } from './components/molecules/hil_modal';
import {
  initChatListeners,
  switchToAgent,
  populateAgentSelect,
  appendStreamingDelta,
  appendThinkingDelta,
  recordTokenUsage,
  updateContextLimitFromModel,
} from './engine/organisms/chat_controller';
import { registerStreamHandlers, registerResearchRouter } from './engine/molecules/event_bus';
import { setLogTransport, flushBufferToTransport, type LogEntry } from './logger';
import * as ResearchModule from './views/research';

// ── Wire event_bus callbacks (engine ← view layer) ──
registerStreamHandlers({
  onDelta: appendStreamingDelta,
  onThinking: appendThinkingDelta,
  onToken: recordTokenUsage,
  onModel: updateContextLimitFromModel,
});
registerResearchRouter({
  isStreaming: ResearchModule.isStreaming,
  getRunId: ResearchModule.getRunId,
  appendDelta: ResearchModule.appendDelta,
  resolveStream: ResearchModule.resolveStream,
});
import {
  initChannels,
  openMemoryFile,
  autoStartConfiguredChannels,
  closeChannelSetup,
} from './views/channels';
import { switchView, showView } from './views/router';
import { initSettingsTabs } from './views/settings-tabs';
import * as SettingsModule from './views/settings-main';
import * as ModelsSettings from './views/settings-models';
import * as AgentDefaultsSettings from './views/settings-agent-defaults';
import * as SessionsSettings from './views/settings-sessions';
import * as VoiceSettings from './views/settings-voice';
import * as SkillsSettings from './views/settings-skills';
import { setConnected as setSettingsConnected } from './views/settings-config';
import { setConnected } from './state/connection';
import * as MemoryPalaceModule from './views/memory-palace';
import * as MailModule from './views/mail';
import * as FoundryModule from './views/foundry';
import * as NodesModule from './views/nodes';
import * as ProjectsModule from './views/projects';
import * as AgentsModule from './views/agents';
import * as TasksModule from './views/tasks';
import * as OrchestratorModule from './views/orchestrator';

// ── Tauri bridge ─────────────────────────────────────────────────────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
    event: {
      listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
    };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const listen = tauriWindow.__TAURI__?.event?.listen;
let unlistenTaskUpdated: (() => void) | null = null;

// ── Global error handlers ──────────────────────────────────────────────────────
function crashLog(msg: string) {
  try {
    const log = JSON.parse(localStorage.getItem('paw-crash-log') || '[]') as string[];
    log.push(`${new Date().toISOString()} ${msg}`);
    while (log.length > 50) log.shift();
    localStorage.setItem('paw-crash-log', JSON.stringify(log));
  } catch {
    /* localStorage might be full */
  }
}
// Wire up the centralized error boundary (replaces inline error handlers)
installErrorBoundary();
setErrorHandler((report) => {
  crashLog(`${report.source}: ${report.message}`);
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
  } catch {
    /* leave as-is */
  }
}
(window as unknown as Record<string, unknown>).__refreshModelLabel = refreshModelLabel;

// ── Engine connection ───────────────────────────────────────────────────────────
async function connectEngine(): Promise<boolean> {
  if (isEngineMode()) {
    console.debug('[main] Engine mode — using Tauri IPC');
    await startEngineBridge();
    appState.wsConnected = true;
    setConnected(true);
    setSettingsConnected(true);

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
    if (chatAvatarEl && initAgent)
      chatAvatarEl.innerHTML = AgentsModule.spriteAvatar(initAgent.avatar, 32);

    refreshModelLabel();
    TasksModule.startCronTimer();
    if (listen) {
      if (unlistenTaskUpdated) {
        unlistenTaskUpdated();
        unlistenTaskUpdated = null;
      }
      listen<{ task_id: string; status: string }>('task-updated', (event) => {
        TasksModule.onTaskUpdated(event.payload);
      }).then((fn) => {
        unlistenTaskUpdated = fn;
      });
    }

    pawEngine
      .autoSetup()
      .then((result) => {
        if (result.action === 'ollama_added') {
          showToast(result.message || `Ollama detected! Using model '${result.model}'.`, 'success');
          ModelsSettings.loadModelsSettings();
        }
      })
      .catch((e) => console.warn('[main] Auto-setup failed (non-fatal):', e));

    pawEngine
      .ensureEmbeddingReady()
      .then((status) => {
        if (status.error) console.warn('[main] Ollama embedding setup:', status.error);
        else
          console.debug(
            `[main] Ollama ready: model=${status.model_name} dims=${status.embedding_dims}`,
          );
      })
      .catch((e) => console.warn('[main] Ollama auto-init failed (non-fatal):', e));

    return true;
  }
  console.warn('[main] connectEngine: engine mode should have handled it above');
  return false;
}

// ── Initialize ──────────────────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
  try {
    console.debug('[main] Paw starting...');

    for (const el of document.querySelectorAll<HTMLElement>('[data-icon]')) {
      const name = el.dataset.icon;
      if (name) el.innerHTML = icon(name);
    }

    initTheme();

    try {
      const prevLog = localStorage.getItem('paw-crash-log');
      if (prevLog) {
        const entries = JSON.parse(prevLog) as string[];
        if (entries.length) entries.slice(-5).forEach((e) => console.warn('  ', e));
      }
    } catch {
      /* ignore */
    }
    crashLog('startup');

    let dbReady = false;
    for (let attempt = 1; attempt <= 3; attempt++) {
      try {
        await initDb();
        dbReady = true;
        break;
      } catch (e) {
        console.warn(`[main] DB init attempt ${attempt}/3 failed:`, e);
        if (attempt < 3) await new Promise((r) => setTimeout(r, 500 * attempt));
      }
    }
    if (!dbReady) {
      const dbBanner = $('db-error-banner');
      if (dbBanner) {
        dbBanner.style.display = 'flex';
        $('db-error-retry')?.addEventListener('click', async () => {
          try {
            await initDb();
            dbBanner.style.display = 'none';
            showToast('Database connected successfully', 'success');
            // Continue the init chain that was skipped
            await initDbEncryption().catch((e) => {
              console.error('[main] OS keychain unavailable after DB retry:', e);
            });
            await initSecuritySettings().catch(() => {});
          } catch (retryErr) {
            const msg = $('db-error-message');
            if (msg)
              msg.textContent = `Retry failed: ${retryErr instanceof Error ? retryErr.message : String(retryErr)}`;
          }
        });
        $('db-error-dismiss')?.addEventListener('click', () => {
          dbBanner.style.display = 'none';
        });
      }
    }

    const encReady = dbReady
      ? await initDbEncryption().catch((e) => {
          console.error('[main] OS keychain unavailable — DB field encryption disabled:', e);
          return false;
        })
      : false;
    if (dbReady) {
      await initSecuritySettings().catch((e) =>
        console.warn('[main] Security settings init failed:', e),
      );
      // Load model pricing overrides from DB
      try {
        const pricingRows = await listModelPricing();
        if (pricingRows.length > 0) {
          applyModelPricingOverrides(pricingRows);
          console.debug(`[main] Loaded ${pricingRows.length} model pricing override(s) from DB`);
        }
      } catch (e) {
        console.warn('[main] Model pricing load failed:', e);
      }
    }

    // ── Persistent file log transport ────────────────────────────────────────
    try {
      const fs = await import('@tauri-apps/plugin-fs');
      const path = await import('@tauri-apps/api/path');
      const logDir = await path.join(await path.homeDir(), 'Documents', 'Paw', 'logs');
      await fs.mkdir(logDir, { recursive: true });
      const today = new Date().toISOString().slice(0, 10); // YYYY-MM-DD
      const logFile = await path.join(logDir, `paw-${today}.log`);

      // Prune log files older than 7 days (best-effort, non-blocking)
      fs.readDir(logDir)
        .then(async (entries) => {
          const cutoff = Date.now() - 7 * 24 * 60 * 60 * 1000;
          for (const entry of entries) {
            if (!entry.name?.endsWith('.log')) continue;
            const m = entry.name.match(/^paw-(\d{4}-\d{2}-\d{2})\.log$/);
            if (m && new Date(m[1]).getTime() < cutoff) {
              await fs.remove(await path.join(logDir, entry.name)).catch(() => {});
            }
          }
        })
        .catch(() => {});

      // Buffer writes and flush periodically to avoid excessive I/O
      let pendingLines: string[] = [];
      let flushTimer: ReturnType<typeof setTimeout> | null = null;

      async function flushToFile() {
        if (pendingLines.length === 0) return;
        const batch = `${pendingLines.join('\n')}\n`;
        pendingLines = [];
        try {
          await fs.writeTextFile(logFile, batch, { append: true });
        } catch {
          /* filesystem write failed — swallow to avoid cascade */
        }
      }

      setLogTransport((_entry: LogEntry, formatted: string) => {
        pendingLines.push(formatted);
        if (!flushTimer) {
          flushTimer = setTimeout(() => {
            flushTimer = null;
            flushToFile();
          }, 1000);
        }
      });

      // Replay any logs emitted before the transport was ready
      flushBufferToTransport();
      await flushToFile(); // ensure replayed entries are written immediately
      console.debug(`[main] File log transport active → ${logFile}`);
    } catch (e) {
      console.warn('[main] File log transport init failed (non-fatal):', e);
    }

    // Show persistent warning banner when encryption is unavailable
    if (!encReady) {
      const encBanner = $('encryption-warning-banner');
      if (encBanner) {
        encBanner.style.display = 'flex';
        $('encryption-warning-dismiss')?.addEventListener('click', () => {
          encBanner.style.display = 'none';
        });
      }
    }

    MemoryPalaceModule.initPalaceEvents();
    window.addEventListener('palace-open-file', (e: Event) => {
      openMemoryFile((e as CustomEvent).detail as string);
    });

    MailModule.configure({
      switchView,
      setCurrentSession: (key) => {
        appState.currentSessionKey = key;
      },
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
      setCurrentAgent: (agentId) => {
        if (agentId) switchToAgent(agentId);
      },
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
    initChatListeners();
    initHILModal();

    console.debug('[main] Pawz engine mode — starting...');
    switchView('today');
    await connectEngine();

    autoStartConfiguredChannels().catch((e) =>
      console.warn('[main] Auto-start channels error:', e),
    );
    console.debug('[main] Pawz initialized');
  } catch (e) {
    console.error('[main] Init error:', e);
    showView('setup-view');
  }
});
