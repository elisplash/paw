// src/views/router.ts
// View routing — switchView and showView — extracted from main.ts.

import { appState } from '../state/index';
import { loadChannels, loadSpaceCron } from './channels';
import { loadSessions, populateAgentSelect } from '../engine/organisms/chat_controller';
import { loadActiveSettingsTab } from './settings-tabs';
import * as AgentsModule from './agents';
import * as AutomationsModule from './automations';
import * as SkillsSettings from './settings-skills';
import * as BuiltInModule from './built-in';
import * as FoundryModule from './foundry';
import * as TasksModule from './tasks';
import * as OrchestratorModule from './orchestrator';
import * as SettingsModule from './settings-main';
import * as MailModule from './mail';
import * as TodayModule from './today';
// PawzHub removed — n8n integration marketplace replaces it
// import * as PawzHubModule from './pawzhub';
import * as IntegrationsModule from './integrations';

export const allViewIds = [
  'setup-view',
  'manual-setup-view',
  'install-view',
  'chat-view',
  'tasks-view',
  'code-view',
  'content-view',
  'mail-view',
  'channels-view',
  'research-view',
  'memory-view',
  'skills-view',
  'builtin-view',
  'foundry-view',
  'settings-view',
  'nodes-view',
  'agents-view',
  'today-view',
  'trading-view',
  'squads-view',
  // 'pawzhub-view', // removed — redirects to integrations
  'integrations-view',
];

const viewMap: Record<string, string> = {
  dashboard: 'today-view',
  chat: 'chat-view',
  tasks: 'tasks-view',
  code: 'tasks-view',
  content: 'content-view',
  mail: 'mail-view',
  automations: 'tasks-view',
  channels: 'channels-view',
  research: 'chat-view',
  memory: 'settings-view',
  skills: 'skills-view',
  builtin: 'builtin-view',
  foundry: 'foundry-view',
  settings: 'settings-view',
  nodes: 'settings-view',
  agents: 'agents-view',
  today: 'today-view',
  orchestrator: 'tasks-view',
  trading: 'today-view',
  squads: 'tasks-view',
  pawzhub: 'integrations-view', // PawzHub merged into Integrations
  integrations: 'integrations-view',
};

/** Check whether the app has been initialised (engine mode active). */
function isConfigured(): boolean {
  return localStorage.getItem('paw-runtime-mode') === 'engine';
}

/** Map deprecated/merged sidebar names to the nav-item that should highlight. */
const navHighlightMap: Record<string, string> = {
  squads: 'tasks',
  code: 'tasks',
  orchestrator: 'tasks',
  automations: 'tasks',
  memory: 'settings',
  nodes: 'settings',
  research: 'chat',
  trading: 'today',
  dashboard: 'today',
};

/** Programmatically click a settings sub-tab (for memory/engine redirects). */
function _switchSettingsTab(tabName: string) {
  const btn = document.querySelector(
    `.settings-tab[data-settings-tab="${tabName}"]`,
  ) as HTMLElement | null;
  if (btn) btn.click();
}

export function switchView(viewName: string) {
  if (!isConfigured() && viewName !== 'settings') return;

  const highlightName = navHighlightMap[viewName] ?? viewName;
  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.getAttribute('data-view') === highlightName);
  });
  allViewIds.forEach((id) => document.getElementById(id)?.classList.remove('active'));
  (document.getElementById(viewMap[viewName] ?? '') ?? null)?.classList.add('active');

  if (appState.wsConnected) {
    switch (viewName) {
      case 'dashboard':
        TodayModule.loadToday();
        break;
      case 'chat':
      case 'research':
        // Research redirects to Chat (research is now a chat mode)
        loadSessions();
        populateAgentSelect();
        break;
      case 'channels':
        loadChannels();
        break;
      case 'automations': {
        // Redirect: automations is now the Scheduled tab inside Tasks
        const al = AgentsModule.getAgents();
        TasksModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        AutomationsModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        TasksModule.loadTasks();
        AutomationsModule.loadCron();
        TasksModule.switchTab('scheduled');
        break;
      }
      case 'agents':
        AgentsModule.loadAgents();
        break;
      case 'today':
      case 'trading':
        // Trading removed from sidebar — redirect to Today
        TodayModule.loadToday();
        break;
      case 'skills':
        SkillsSettings.loadSkillsSettings();
        break;
      case 'builtin':
        BuiltInModule.loadBuiltIn();
        break;
      case 'foundry':
        FoundryModule.loadModels();
        FoundryModule.loadModes();
        break;
      case 'nodes':
        // Engine moved to Settings → Engine tab
        SettingsModule.loadSettings();
        SettingsModule.startUsageAutoRefresh();
        loadActiveSettingsTab();
        _switchSettingsTab('engine');
        break;
      case 'memory':
        // Memory moved to Settings → Memory tab
        SettingsModule.loadSettings();
        SettingsModule.startUsageAutoRefresh();
        loadActiveSettingsTab();
        _switchSettingsTab('memory');
        break;
      case 'tasks':
      case 'code': {
        const al = AgentsModule.getAgents();
        TasksModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        AutomationsModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        TasksModule.loadTasks();
        AutomationsModule.loadCron();
        break;
      }
      case 'squads': {
        // Squads merged into Tasks → Squads tab
        const al2 = AgentsModule.getAgents();
        TasksModule.setAgents(al2.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        AutomationsModule.setAgents(al2.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        TasksModule.loadTasks();
        AutomationsModule.loadCron();
        TasksModule.switchTab('squads');
        break;
      }
      case 'orchestrator':
        OrchestratorModule.loadProjects();
        TasksModule.switchTab('projects');
        break;
      case 'mail':
        MailModule.loadMail();
        loadSpaceCron('mail');
        break;
      case 'settings':
        SettingsModule.loadSettings();
        SettingsModule.startUsageAutoRefresh();
        loadActiveSettingsTab();
        break;
      default:
        break;
    }
  }
  if (viewName !== 'settings' && viewName !== 'memory' && viewName !== 'nodes')
    SettingsModule.stopUsageAutoRefresh();
  if (viewName !== 'settings' && viewName !== 'memory' && viewName !== 'nodes')
    SettingsModule.stopOverrideBannerInterval();
  if (viewName !== 'orchestrator') OrchestratorModule.stopMessagePoll();
  switch (viewName) {
    case 'pawzhub':
      // PawzHub removed — redirect to integrations
      IntegrationsModule.loadIntegrations();
      break;
    case 'integrations':
      IntegrationsModule.loadIntegrations();
      break;
    default:
      break;
  }
  if (viewName === 'settings') SettingsModule.loadSettings();
}

export function showView(viewId: string) {
  allViewIds.forEach((id) => document.getElementById(id)?.classList.remove('active'));
  document.getElementById(viewId)?.classList.add('active');
}

// Wire nav clicks immediately on module load.
document.querySelectorAll('[data-view]').forEach((item) => {
  item.addEventListener('click', () => {
    const view = item.getAttribute('data-view');
    if (view) switchView(view);
  });
});
