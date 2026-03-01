// src/components/agents-panel.ts — Agents view side panel + template marketplace logic
// Vanilla TS, no React. Populates the hero stats, side panel cards, and template grid.

import { kineticRow, kineticStagger } from './kinetic-row';
import { escHtml } from './helpers';

// ── Agent Template Catalog ─────────────────────────────────────────────

export interface AgentTemplate {
  id: string;
  name: string;
  icon: string; // Material Symbol name
  desc: string;
  category:
    | 'productivity'
    | 'engineering'
    | 'creative'
    | 'data'
    | 'communication'
    | 'security'
    | 'trading';
  model: string; // Recommended model
  skills: string[];
  systemPrompt: string;
  personality: { tone: string; initiative: string; detail: string };
  popular?: boolean;
}

export const AGENT_TEMPLATE_CATALOG: AgentTemplate[] = [
  // ── Productivity ──
  {
    id: 'exec-assistant',
    name: 'Executive Assistant',
    icon: 'work',
    desc: 'Calendar management, email triage, meeting prep, and daily briefings',
    category: 'productivity',
    model: 'default',
    skills: ['web_search', 'read_file', 'write_file'],
    systemPrompt:
      'You are an executive assistant. Prepare meeting agendas, organize documents, and provide daily briefings. Be proactive about scheduling conflicts and follow-ups.',
    personality: { tone: 'formal', initiative: 'proactive', detail: 'thorough' },
    popular: true,
  },
  {
    id: 'project-manager',
    name: 'Project Manager',
    icon: 'assignment',
    desc: 'Track tasks, deadlines, blockers, and generate status reports',
    category: 'productivity',
    model: 'default',
    skills: ['create_task', 'list_tasks', 'manage_task', 'web_search', 'read_file', 'write_file'],
    systemPrompt:
      'You are a project manager. Track tasks, identify blockers, manage deadlines, and generate status reports. Use structured formats for clarity.',
    personality: { tone: 'balanced', initiative: 'proactive', detail: 'thorough' },
  },
  {
    id: 'note-taker',
    name: 'Meeting Scribe',
    icon: 'edit_note',
    desc: 'Summarize meetings, extract action items, and distribute notes',
    category: 'productivity',
    model: 'default',
    skills: ['write_file', 'read_file'],
    systemPrompt:
      'You are a meeting note-taker. Summarize discussions, extract action items with owners and deadlines, and format notes clearly. Always ask for the meeting context first.',
    personality: { tone: 'formal', initiative: 'reactive', detail: 'thorough' },
  },

  // ── Engineering ──
  {
    id: 'code-reviewer',
    name: 'Code Reviewer',
    icon: 'rate_review',
    desc: 'Review PRs, suggest improvements, catch bugs and security issues',
    category: 'engineering',
    model: 'default',
    skills: ['read_file', 'list_directory', 'web_search', 'exec'],
    systemPrompt:
      'You are a senior code reviewer. Analyze code for bugs, security issues, performance problems, and style. Provide actionable suggestions with examples. Be thorough but constructive.',
    personality: { tone: 'balanced', initiative: 'proactive', detail: 'thorough' },
    popular: true,
  },
  {
    id: 'devops-engineer',
    name: 'DevOps Engineer',
    icon: 'cloud_sync',
    desc: 'CI/CD pipelines, Docker, Kubernetes, infrastructure as code',
    category: 'engineering',
    model: 'default',
    skills: ['exec', 'read_file', 'write_file', 'list_directory', 'web_search'],
    systemPrompt:
      'You are a DevOps engineer specializing in CI/CD, containerization, and cloud infrastructure. Help with Docker, Kubernetes, Terraform, and deployment pipelines. Always consider security and scalability.',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'thorough' },
  },
  {
    id: 'full-stack-dev',
    name: 'Full-Stack Dev',
    icon: 'code',
    desc: 'Build features across frontend and backend with modern frameworks',
    category: 'engineering',
    model: 'default',
    skills: ['read_file', 'write_file', 'exec', 'list_directory', 'web_search', 'web_read'],
    systemPrompt:
      'You are a full-stack developer. Write clean, tested, production-ready code. Use modern best practices. Explain architecture decisions. Handle both frontend (React, Vue, vanilla) and backend (Node, Python, Rust).',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'thorough' },
    popular: true,
  },
  {
    id: 'api-architect',
    name: 'API Architect',
    icon: 'api',
    desc: 'Design RESTful APIs, GraphQL schemas, and integration patterns',
    category: 'engineering',
    model: 'default',
    skills: ['read_file', 'write_file', 'web_search', 'rest_api_call', 'exec'],
    systemPrompt:
      'You are an API architect. Design clean, well-documented APIs following REST or GraphQL best practices. Consider versioning, pagination, error handling, and authentication patterns.',
    personality: { tone: 'formal', initiative: 'balanced', detail: 'thorough' },
  },

  // ── Creative ──
  {
    id: 'content-writer',
    name: 'Content Writer',
    icon: 'draw',
    desc: 'Blog posts, documentation, marketing copy, and social media',
    category: 'creative',
    model: 'default',
    skills: ['web_search', 'web_read', 'write_file', 'read_file'],
    systemPrompt:
      'You are a professional content writer. Create engaging, well-structured content for blogs, docs, and marketing. Adapt tone to the audience. Use storytelling techniques and clear CTAs where appropriate.',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'balanced' },
    popular: true,
  },
  {
    id: 'ux-designer',
    name: 'UX Designer',
    icon: 'design_services',
    desc: 'User flows, wireframes, accessibility audits, and design systems',
    category: 'creative',
    model: 'default',
    skills: ['web_search', 'web_read', 'web_screenshot', 'write_file'],
    systemPrompt:
      'You are a UX designer. Create user flows, suggest UI improvements, audit accessibility, and help build design systems. Focus on user-centered design principles and WCAG compliance.',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'balanced' },
  },
  {
    id: 'copyeditor',
    name: 'Copy Editor',
    icon: 'spellcheck',
    desc: 'Proofread, edit for clarity, check style guides, and improve readability',
    category: 'creative',
    model: 'default',
    skills: ['read_file', 'write_file'],
    systemPrompt:
      'You are a professional copy editor. Proofread for grammar, spelling, punctuation, and style consistency. Improve clarity and readability. Flag ambiguous phrasing and suggest alternatives.',
    personality: { tone: 'formal', initiative: 'reactive', detail: 'thorough' },
  },

  // ── Data ──
  {
    id: 'data-analyst',
    name: 'Data Analyst',
    icon: 'query_stats',
    desc: 'SQL queries, data visualization, statistical analysis, and reports',
    category: 'data',
    model: 'default',
    skills: ['exec', 'read_file', 'write_file', 'web_search'],
    systemPrompt:
      'You are a data analyst. Write SQL queries, analyze datasets, create visualizations, and generate reports. Use statistical methods appropriately. Always explain your methodology and findings clearly.',
    personality: { tone: 'formal', initiative: 'balanced', detail: 'thorough' },
    popular: true,
  },
  {
    id: 'research-analyst',
    name: 'Research Analyst',
    icon: 'biotech',
    desc: 'Deep web research, competitive analysis, and literature reviews',
    category: 'data',
    model: 'default',
    skills: ['web_search', 'web_read', 'web_browse', 'write_file', 'read_file'],
    systemPrompt:
      'You are a research analyst. Conduct thorough research, synthesize findings from multiple sources, and present structured reports. Always cite sources and distinguish facts from opinions.',
    personality: { tone: 'formal', initiative: 'proactive', detail: 'thorough' },
  },

  // ── Communication ──
  {
    id: 'community-manager',
    name: 'Community Manager',
    icon: 'forum',
    desc: 'Discord/Slack moderation, engagement, and community health monitoring',
    category: 'communication',
    model: 'default',
    skills: ['web_search', 'write_file'],
    systemPrompt:
      'You are a community manager. Monitor channels, engage with members, answer questions, moderate discussions, and track community health metrics. Be warm, inclusive, and proactive.',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'balanced' },
  },

  // ── Security ──
  {
    id: 'security-auditor',
    name: 'Security Auditor',
    icon: 'shield',
    desc: 'Vulnerability scanning, dependency audits, and security best practices',
    category: 'security',
    model: 'default',
    skills: ['exec', 'read_file', 'list_directory', 'web_search'],
    systemPrompt:
      'You are a security auditor. Scan code for vulnerabilities, audit dependencies, check configurations, and recommend security best practices. Follow OWASP guidelines. Always prioritize findings by severity.',
    personality: { tone: 'formal', initiative: 'proactive', detail: 'thorough' },
    popular: true,
  },

  // ── Trading ──
  {
    id: 'trading-analyst',
    name: 'Trading Analyst',
    icon: 'trending_up',
    desc: 'Market analysis, portfolio tracking, and trading strategy research',
    category: 'trading',
    model: 'default',
    skills: [
      'coinbase_prices',
      'coinbase_balance',
      'sol_balance',
      'sol_portfolio',
      'web_search',
      'web_read',
    ],
    systemPrompt:
      'You are a trading analyst. Monitor markets, analyze price action, track portfolios, and research trading strategies. Always include risk disclaimers. Never give financial advice — present data and analysis only.',
    personality: { tone: 'balanced', initiative: 'proactive', detail: 'thorough' },
  },
  {
    id: 'defi-scout',
    name: 'DeFi Scout',
    icon: 'explore',
    desc: 'Monitor DeFi protocols, yield opportunities, and token launches',
    category: 'trading',
    model: 'default',
    skills: [
      'dex_trending',
      'dex_token_info',
      'dex_check_token',
      'sol_token_info',
      'web_search',
      'web_read',
    ],
    systemPrompt:
      'You are a DeFi scout. Monitor decentralized finance protocols, track yield opportunities, analyze new token launches, and flag potential risks. Always verify contract safety before recommending.',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'thorough' },
  },
];

// ── Category metadata ──────────────────────────────────────────────────

const CATEGORY_META: Record<string, { icon: string; label: string; color: string }> = {
  productivity: { icon: 'work', label: 'Productivity', color: 'var(--accent)' },
  engineering: { icon: 'code', label: 'Engineering', color: '#8b5cf6' },
  creative: { icon: 'palette', label: 'Creative', color: '#ec4899' },
  data: { icon: 'query_stats', label: 'Data & Research', color: '#06b6d4' },
  communication: { icon: 'forum', label: 'Communication', color: '#10b981' },
  security: { icon: 'shield', label: 'Security', color: '#ef4444' },
  trading: { icon: 'trending_up', label: 'Trading', color: '#f59e0b' },
};

// ── Render functions ───────────────────────────────────────────────────

/** Render the hero stats counters */
export function updateAgentsHeroStats(agents: { lastUsed?: string; model?: string }[]) {
  const total = agents.length;
  const active = agents.filter(
    (a) => a.lastUsed && Date.now() - new Date(a.lastUsed).getTime() < 600000,
  ).length;
  const models = new Set(agents.map((a) => a.model).filter(Boolean)).size;

  const elTotal = document.getElementById('agents-stat-total');
  const elActive = document.getElementById('agents-stat-active');
  const elModels = document.getElementById('agents-stat-models');

  if (elTotal) elTotal.textContent = String(total);
  if (elActive) elActive.textContent = String(active);
  if (elModels) elModels.textContent = String(models);
}

/** Render capabilities summary in side panel */
export function renderCapabilitiesList(agents: { skills: string[] }[]) {
  const el = document.getElementById('agents-capabilities-list');
  if (!el) return;

  // Count unique skills across all agents
  const skillCounts = new Map<string, number>();
  agents.forEach((a) => a.skills.forEach((s) => skillCounts.set(s, (skillCounts.get(s) || 0) + 1)));

  const topSkills = [...skillCounts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8);

  if (topSkills.length === 0) {
    el.innerHTML = '<div class="agents-cap-empty">No skills assigned yet</div>';
    return;
  }

  el.innerHTML = topSkills
    .map(([skill, count]) => {
      const pct = Math.round((count / agents.length) * 100);
      return `<div class="agents-cap-row">
      <span class="agents-cap-name">${escHtml(_formatSkillName(skill))}</span>
      <div class="agents-cap-bar"><div class="agents-cap-fill" style="width:${pct}%"></div></div>
      <span class="agents-cap-count">${count}</span>
    </div>`;
    })
    .join('');
}

/** Render recent activity in side panel */
export function renderActivityList(agents: { name: string; lastUsed?: string }[]) {
  const el = document.getElementById('agents-activity-list');
  if (!el) return;

  const recent = agents
    .filter((a) => a.lastUsed)
    .sort((a, b) => new Date(b.lastUsed!).getTime() - new Date(a.lastUsed!).getTime())
    .slice(0, 5);

  if (recent.length === 0) {
    el.innerHTML = '<div class="agents-activity-empty">No activity yet</div>';
    return;
  }

  el.innerHTML = recent
    .map((a) => {
      const ago = _timeAgo(new Date(a.lastUsed!));
      return `<div class="agents-activity-row">
      <span class="ms agents-activity-icon">smart_toy</span>
      <span class="agents-activity-name">${escHtml(a.name)}</span>
      <span class="agents-activity-time">${ago}</span>
    </div>`;
    })
    .join('');
}

/** Render the template marketplace grid */
export function renderTemplateGrid(onInstall: (templateId: string) => void) {
  const el = document.getElementById('agents-templates-grid');
  if (!el) return;

  // Group by category
  const categories = new Map<string, AgentTemplate[]>();
  AGENT_TEMPLATE_CATALOG.forEach((t) => {
    const list = categories.get(t.category) || [];
    list.push(t);
    categories.set(t.category, list);
  });

  let html = '';
  categories.forEach((templates, cat) => {
    const meta = CATEGORY_META[cat] || { icon: 'category', label: cat, color: 'var(--text-muted)' };
    html += `<div class="agents-tpl-category">
      <div class="agents-tpl-cat-header">
        <span class="ms agents-tpl-cat-icon" style="color:${meta.color}">${meta.icon}</span>
        <span class="agents-tpl-cat-label">${meta.label}</span>
      </div>
      <div class="agents-tpl-cat-cards">
        ${templates
          .map(
            (t) => `
        <div class="agents-tpl-card k-row k-spring" data-template-id="${t.id}">
          ${t.popular ? '<span class="agents-tpl-popular">Popular</span>' : ''}
          <span class="ms agents-tpl-card-icon" style="color:${meta.color}">${t.icon}</span>
          <div class="agents-tpl-card-name">${escHtml(t.name)}</div>
          <div class="agents-tpl-card-desc">${escHtml(t.desc)}</div>
          <button class="agents-tpl-install-btn" data-tpl-id="${t.id}">
            <span class="ms ms-sm">download</span> Install
          </button>
        </div>`,
          )
          .join('')}
      </div>
    </div>`;
  });

  el.innerHTML = html;

  // Bind install buttons
  el.querySelectorAll('.agents-tpl-install-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = (btn as HTMLElement).getAttribute('data-tpl-id');
      if (id) onInstall(id);
    });
  });

  // Apply kinetic to template cards
  el.querySelectorAll('.agents-tpl-card').forEach((card) => {
    kineticRow(card as HTMLElement, { spring: true });
  });
}

/** Apply kinetic animations to all agents page sections */
export function initAgentsKinetic() {
  // Stagger the side panel cards
  const sidePanel = document.querySelector('.agents-side-panel');
  if (sidePanel) kineticStagger(sidePanel as HTMLElement, '.agents-panel-card');

  // Materialise the section cards
  document.querySelectorAll('.agents-section.k-materialise').forEach((el) => {
    kineticRow(el as HTMLElement, { materialise: true });
  });

  // Spring on hero stats
  document.querySelectorAll('.agents-hero-stat.k-spring').forEach((el) => {
    kineticRow(el as HTMLElement, { spring: true });
  });

  // Spring on panel cards
  document.querySelectorAll('.agents-panel-card.k-spring').forEach((el) => {
    kineticRow(el as HTMLElement, { spring: true, materialise: true });
  });
}

// ── Helpers ────────────────────────────────────────────────────────────

function _formatSkillName(id: string): string {
  return id.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function _timeAgo(d: Date): string {
  const diff = Date.now() - d.getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  if (days === 1) return 'yesterday';
  return `${days}d ago`;
}
