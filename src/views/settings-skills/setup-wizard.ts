// My Skills — Setup Wizard (Phase 4)
// One-time onboarding category picker that lets users choose which
// skill categories to enable on first launch.
//
// Atomic pattern: this module owns rendering + event binding for the wizard.

import { pawEngine, type EngineSkillStatus } from '../../engine';
import { showToast } from '../../components/toast';

// ── Category definitions ───────────────────────────────────────────────────

interface WizardCategory {
  id: string;
  label: string;
  icon: string; // Material Symbol
  description: string;
  /** Skill categories from the backend that map to this wizard category */
  backendCategories: string[];
}

const WIZARD_CATEGORIES: WizardCategory[] = [
  {
    id: 'communication',
    label: 'Communication',
    icon: 'chat',
    description: 'Discord, Telegram, WhatsApp, iMessage',
    backendCategories: ['vault', 'communication'],
  },
  {
    id: 'productivity',
    label: 'Productivity',
    icon: 'task_alt',
    description: 'Notes, reminders, Notion',
    backendCategories: ['productivity', 'api'],
  },
  {
    id: 'development',
    label: 'Development',
    icon: 'code',
    description: 'tmux, session logs, terminal tools',
    backendCategories: ['development'],
  },
  {
    id: 'smart-home',
    label: 'Smart Home',
    icon: 'home',
    description: 'Philips Hue, Sonos, Eight Sleep, cameras',
    backendCategories: ['smarthome'],
  },
  {
    id: 'media',
    label: 'Media',
    icon: 'music_note',
    description: 'Spotify, Whisper, image gen, video, TTS, GIFs',
    backendCategories: ['media'],
  },
  {
    id: 'finance',
    label: 'Finance & Trading',
    icon: 'account_balance',
    description: 'Trading integrations via n8n',
    backendCategories: [], // Handled by specific skill IDs
  },
  {
    id: 'general',
    label: 'General',
    icon: 'auto_awesome',
    description: 'Weather, search, summarize, security audit',
    backendCategories: ['cli', 'system'],
  },
];

// Finance skills don't have their own backend category — they're under "vault"
// so we use explicit IDs.
const FINANCE_SKILL_IDS: string[] = [];

// Communication is also under "vault" but we distinguish by skill IDs
const COMMUNICATION_SKILL_IDS = ['telegram', 'discord', 'whatsapp', 'imessage', 'webhook'];

// ── Render ──────────────────────────────────────────────────────────────────

export function renderSetupWizard(): string {
  const categoryCards = WIZARD_CATEGORIES.map(
    (cat) => `
    <label class="sw-category-card" data-category="${cat.id}">
      <input type="checkbox" class="sw-category-check" value="${cat.id}" />
      <div class="sw-category-content">
        <span class="ms sw-category-icon">${cat.icon}</span>
        <div class="sw-category-text">
          <strong>${cat.label}</strong>
          <span class="sw-category-desc">${cat.description}</span>
        </div>
      </div>
    </label>
  `,
  ).join('');

  return `
  <div class="sw-overlay">
    <div class="sw-dialog">
      <div class="sw-header">
        <span class="ms sw-logo">auto_awesome</span>
        <h2>Welcome to OpenPawz</h2>
        <p>What do you want your agent to help with? Pick a few categories to get started. You can always change this later.</p>
      </div>

      <div class="sw-categories">
        ${categoryCards}
      </div>

      <div class="sw-footer">
        <button class="btn btn-ghost sw-skip-btn">Skip — enable defaults only</button>
        <button class="btn btn-primary sw-start-btn" disabled>
          <span class="ms ms-sm">rocket_launch</span>
          Get Started
        </button>
      </div>
    </div>
  </div>`;
}

// ── Event binding ──────────────────────────────────────────────────────────

export function bindSetupWizardEvents(
  skills: EngineSkillStatus[],
  onComplete: () => Promise<void>,
): void {
  const startBtn = document.querySelector('.sw-start-btn') as HTMLButtonElement | null;
  const skipBtn = document.querySelector('.sw-skip-btn') as HTMLButtonElement | null;
  const checkboxes = document.querySelectorAll(
    '.sw-category-check',
  ) as NodeListOf<HTMLInputElement>;

  // Toggle card visual + enable/disable start button
  checkboxes.forEach((cb) => {
    cb.addEventListener('change', () => {
      const card = cb.closest('.sw-category-card');
      if (card) card.classList.toggle('sw-selected', cb.checked);
      if (startBtn) {
        const anyChecked = Array.from(checkboxes).some((c) => c.checked);
        startBtn.disabled = !anyChecked;
      }
    });
  });

  // Get Started → bulk enable selected categories
  startBtn?.addEventListener('click', async () => {
    startBtn.disabled = true;
    startBtn.innerHTML =
      '<span class="wa-spinner" style="width:14px;height:14px"></span> Setting up...';

    const selectedCategories = Array.from(checkboxes)
      .filter((c) => c.checked)
      .map((c) => c.value);

    const skillIds = resolveSkillIds(skills, selectedCategories);

    try {
      // Bulk enable chosen skills
      await pawEngine.skillBulkEnable(skillIds, true);
      // Mark onboarding complete
      await pawEngine.setOnboardingComplete();
      showToast(`${skillIds.length} skills enabled! Your workspace is ready.`, 'success');
      // Remove wizard overlay
      document.querySelector('.sw-overlay')?.remove();
      // Reload the skills view
      await onComplete();
    } catch (err) {
      showToast(`Setup failed: ${err}`, 'error');
      startBtn.disabled = false;
      startBtn.innerHTML = '<span class="ms ms-sm">rocket_launch</span> Get Started';
    }
  });

  // Skip → just enable defaults (weather, blogwatcher) + mark complete
  skipBtn?.addEventListener('click', async () => {
    skipBtn.disabled = true;
    skipBtn.textContent = 'Setting up...';

    try {
      await pawEngine.setOnboardingComplete();
      document.querySelector('.sw-overlay')?.remove();
      await onComplete();
    } catch (err) {
      showToast(`Setup failed: ${err}`, 'error');
      skipBtn.disabled = false;
      skipBtn.textContent = 'Skip — enable defaults only';
    }
  });
}

// ── Helpers ────────────────────────────────────────────────────────────────

/** Resolve wizard category selections into an array of skill IDs to enable. */
function resolveSkillIds(skills: EngineSkillStatus[], selectedCategories: string[]): string[] {
  const ids = new Set<string>();

  for (const catId of selectedCategories) {
    const wizardCat = WIZARD_CATEGORIES.find((c) => c.id === catId);
    if (!wizardCat) continue;

    if (catId === 'finance') {
      // Finance skills are under vault category — use explicit IDs
      for (const id of FINANCE_SKILL_IDS) ids.add(id);
    } else if (catId === 'communication') {
      // Communication-related vault skills
      for (const id of COMMUNICATION_SKILL_IDS) ids.add(id);
    } else {
      // Match by backend category
      for (const skill of skills) {
        if (wizardCat.backendCategories.includes(skill.category)) {
          // Under productivity/api, skip finance/communication vault skills
          ids.add(skill.id);
        }
      }
    }
  }

  // Always include essential defaults
  ids.add('weather');
  ids.add('blogwatcher');

  return Array.from(ids);
}
