// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Template Browser Molecules
// Template gallery with search, filtering, and instantiation.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowTemplate,
  type FlowTemplateCategory,
  TEMPLATE_CATEGORIES,
  filterTemplates,
} from './atoms';
import { escAttr } from './molecule-state';

// ── Template Browser State ─────────────────────────────────────────────────

let _templateCategory: FlowTemplateCategory | 'all' = 'all';
let _templateQuery = '';

// ── Template Browser Rendering ─────────────────────────────────────────────

export function renderTemplateBrowser(
  container: HTMLElement,
  templates: FlowTemplate[],
  onInstantiate: (tpl: FlowTemplate) => void,
) {
  const filtered = filterTemplates(templates, _templateCategory, _templateQuery);
  const categories = Object.entries(TEMPLATE_CATEGORIES) as [
    FlowTemplateCategory,
    { label: string; icon: string; color: string },
  ][];

  container.innerHTML = `
    <div class="flow-tpl-browser">
      <div class="flow-tpl-header">
        <span class="ms" style="font-size:18px;color:var(--kinetic-red)">dashboard_customize</span>
        <span class="flow-tpl-title">Templates</span>
        <span class="flow-tpl-count">${filtered.length}</span>
      </div>
      <div class="flow-tpl-search">
        <input type="text" class="flow-tpl-search-input" placeholder="Search templates…" value="${escAttr(_templateQuery)}" />
      </div>
      <div class="flow-tpl-categories">
        <button class="flow-tpl-cat-btn${_templateCategory === 'all' ? ' active' : ''}" data-cat="all">All</button>
        ${categories
          .map(
            ([key, meta]) => `
          <button class="flow-tpl-cat-btn${_templateCategory === key ? ' active' : ''}" data-cat="${key}" title="${meta.label}">
            <span class="ms" style="font-size:14px;color:${meta.color}">${meta.icon}</span>
            ${meta.label}
          </button>
        `,
          )
          .join('')}
      </div>
      <div class="flow-tpl-list">
        ${
          filtered.length === 0
            ? '<div class="flow-tpl-empty">No templates match</div>'
            : filtered
                .map((tpl) => {
                  const catMeta = TEMPLATE_CATEGORIES[tpl.category];
                  return `
            <div class="flow-tpl-card" data-tpl-id="${tpl.id}">
              <div class="flow-tpl-card-header">
                <span class="ms flow-tpl-card-icon" style="color:${catMeta.color}">${tpl.icon}</span>
                <div class="flow-tpl-card-meta">
                  <span class="flow-tpl-card-name">${tpl.name}</span>
                  <span class="flow-tpl-card-cat">${catMeta.label}</span>
                </div>
              </div>
              <p class="flow-tpl-card-desc">${tpl.description}</p>
              <div class="flow-tpl-card-tags">
                ${tpl.tags
                  .slice(0, 3)
                  .map((t) => `<span class="flow-tpl-tag">${t}</span>`)
                  .join('')}
                <span class="flow-tpl-card-nodes">${tpl.nodes.length} integrations</span>
              </div>
              <button class="flow-tpl-use-btn" data-tpl-id="${tpl.id}">Use Template</button>
            </div>
          `;
                })
                .join('')
        }
      </div>
    </div>
  `;

  // Search input
  const searchInput = container.querySelector('.flow-tpl-search-input') as HTMLInputElement | null;
  searchInput?.addEventListener('input', () => {
    _templateQuery = searchInput.value;
    renderTemplateBrowser(container, templates, onInstantiate);
  });

  // Category buttons
  container.querySelectorAll('.flow-tpl-cat-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      _templateCategory = (btn as HTMLElement).dataset.cat as FlowTemplateCategory | 'all';
      renderTemplateBrowser(container, templates, onInstantiate);
    });
  });

  // Use template buttons
  container.querySelectorAll('.flow-tpl-use-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const id = (btn as HTMLElement).dataset.tplId!;
      const tpl = templates.find((t) => t.id === id);
      if (tpl) onInstantiate(tpl);
    });
  });

  // Card click also instantiates
  container.querySelectorAll('.flow-tpl-card').forEach((card) => {
    card.addEventListener('click', () => {
      const id = (card as HTMLElement).dataset.tplId!;
      const tpl = templates.find((t) => t.id === id);
      if (tpl) onInstantiate(tpl);
    });
  });
}
