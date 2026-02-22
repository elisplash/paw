// src/views/settings-tabs.ts
// Settings tab bar wiring — extracted from main.ts.

import * as ModelsSettings from './settings-models';
import * as AgentDefaultsSettings from './settings-agent-defaults';
import * as SessionsSettings from './settings-sessions';
import * as VoiceSettings from './settings-voice';
import * as SkillsSettings from './settings-skills';
import * as BrowserSettings from './settings-browser';
import * as TailscaleSettings from './settings-tailscale';
import * as WebhookSettings from './settings-webhook';
import * as McpSettings from './settings-mcp';
import * as LogsSettings from './settings-logs';

import { $ } from '../components/helpers';

let _activeSettingsTab = 'general';

export function loadActiveSettingsTab() {
  switch (_activeSettingsTab) {
    case 'models':
      ModelsSettings.loadModelsSettings();
      break;
    case 'agent-defaults':
      AgentDefaultsSettings.loadAgentDefaultsSettings();
      break;
    case 'sessions':
      SessionsSettings.loadSessionsSettings();
      break;
    case 'voice':
      VoiceSettings.loadVoiceSettings();
      break;
    case 'skills':
      SkillsSettings.loadSkillsSettings();
      break;
    case 'browser':
      BrowserSettings.loadBrowserSettings();
      break;
    case 'tailscale':
      TailscaleSettings.loadTailscaleSettings();
      break;
    case 'webhook':
      WebhookSettings.loadWebhookSettings();
      break;
    case 'mcp':
      McpSettings.loadMcpSettings();
      break;
    case 'logs':
      LogsSettings.loadLogsSettings();
      break;
    default:
      break;
  }
}

export function initSettingsTabs() {
  const bar = $('settings-tab-bar');
  if (!bar) return;
  bar.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.settings-tab') as HTMLElement | null;
    if (!btn) return;
    const tab = btn.dataset.settingsTab;
    if (!tab || tab === _activeSettingsTab) return;
    bar.querySelectorAll('.settings-tab').forEach((b) => b.classList.remove('active'));
    btn.classList.add('active');
    document.querySelectorAll('.settings-tab-panel').forEach((p) => {
      (p as HTMLElement).style.display = 'none';
    });
    const panel = $(`settings-panel-${tab}`);
    if (panel) panel.style.display = '';
    _activeSettingsTab = tab;
    loadActiveSettingsTab();
  });
}
