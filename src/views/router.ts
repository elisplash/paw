// src/views/router.ts
// View routing — switchView and showView — extracted from main.ts.

import { appState } from '../state/index';
import { loadDashboardCron, loadChannels, loadSpaceCron, loadMemory } from './channels';
import { loadSessions, populateAgentSelect } from '../engine/organisms/chat_controller';
import { loadContentDocs } from './content';
import { loadActiveSettingsTab } from './settings-tabs';
import * as AgentsModule from './agents';
import * as AutomationsModule from './automations';
import * as MemoryPalaceModule from './memory-palace';
import * as SkillsSettings from './settings-skills';
import * as FoundryModule from './foundry';
import * as NodesModule from './nodes';
import * as TasksModule from './tasks';
import * as OrchestratorModule from './orchestrator';
import * as TradingModule from './trading';
import * as SettingsModule from './settings-main';
import * as ResearchModule from './research';
import * as MailModule from './mail';
import * as TodayModule from './today';
import * as ProjectsModule from './projects';

export const allViewIds = [
  'dashboard-view',
  'setup-view',
  'manual-setup-view',
  'install-view',
  'chat-view',
  'tasks-view',
  'code-view',
  'content-view',
  'mail-view',
  'automations-view',
  'channels-view',
  'research-view',
  'memory-view',
  'skills-view',
  'foundry-view',
  'settings-view',
  'nodes-view',
  'agents-view',
  'today-view',
  'orchestrator-view',
  'trading-view',
];

const viewMap: Record<string, string> = {
  dashboard: 'dashboard-view',
  chat: 'chat-view',
  tasks: 'tasks-view',
  code: 'code-view',
  content: 'content-view',
  mail: 'mail-view',
  automations: 'automations-view',
  channels: 'channels-view',
  research: 'research-view',
  memory: 'memory-view',
  skills: 'skills-view',
  foundry: 'foundry-view',
  settings: 'settings-view',
  nodes: 'nodes-view',
  agents: 'agents-view',
  today: 'today-view',
  orchestrator: 'orchestrator-view',
  trading: 'trading-view',
};

/** Read configured state from localStorage without holding a shared pointer. */
function isConfigured(): boolean {
  try {
    const saved = localStorage.getItem('claw-config');
    if (saved) return (JSON.parse(saved) as { configured?: boolean }).configured === true;
  } catch {
    /* */
  }
  return false;
}

export function switchView(viewName: string) {
  if (!isConfigured() && viewName !== 'settings') return;

  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.getAttribute('data-view') === viewName);
  });
  allViewIds.forEach((id) => document.getElementById(id)?.classList.remove('active'));
  (document.getElementById(viewMap[viewName] ?? '') ?? null)?.classList.add('active');

  if (appState.wsConnected) {
    switch (viewName) {
      case 'dashboard':
        loadDashboardCron();
        break;
      case 'chat':
        loadSessions();
        populateAgentSelect();
        break;
      case 'channels':
        loadChannels();
        break;
      case 'automations': {
        const al = AgentsModule.getAgents();
        AutomationsModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        AutomationsModule.loadCron();
        break;
      }
      case 'agents':
        AgentsModule.loadAgents();
        break;
      case 'today':
        TodayModule.loadToday();
        break;
      case 'skills':
        SkillsSettings.loadSkillsSettings();
        break;
      case 'foundry':
        FoundryModule.loadModels();
        FoundryModule.loadModes();
        break;
      case 'nodes':
        NodesModule.loadNodes();
        NodesModule.loadPairingRequests();
        break;
      case 'memory':
        MemoryPalaceModule.loadMemoryPalace();
        loadMemory();
        break;
      case 'tasks': {
        const al = AgentsModule.getAgents();
        TasksModule.setAgents(al.map((a) => ({ id: a.id, name: a.name, avatar: a.avatar })));
        TasksModule.loadTasks();
        break;
      }
      case 'orchestrator':
        OrchestratorModule.loadProjects();
        break;
      case 'trading':
        TradingModule.loadTrading();
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
  if (viewName !== 'settings') SettingsModule.stopUsageAutoRefresh();
  if (viewName !== 'settings') SettingsModule.stopOverrideBannerInterval();
  if (viewName !== 'orchestrator') OrchestratorModule.stopMessagePoll();
  switch (viewName) {
    case 'content':
      loadContentDocs();
      if (appState.wsConnected) loadSpaceCron('content');
      break;
    case 'research':
      ResearchModule.loadResearchProjects();
      if (appState.wsConnected) loadSpaceCron('research');
      break;
    case 'code':
      ProjectsModule.loadProjects();
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
