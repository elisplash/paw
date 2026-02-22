// activity.ts — Activity feed molecule for the Today view sidebar
// Fetches global activity from the engine and renders it into the sidebar card

import { pawEngine, type EngineTaskActivity } from '../../engine';
import { escHtml } from '../../components/helpers';
import { activityIcon, relativeTime, truncateContent } from './atoms';

const $ = (id: string) => document.getElementById(id);

/** Fetch recent global activity and render into #today-activity. */
export async function fetchAndRenderActivity() {
  const container = $('today-activity');
  if (!container) return;

  try {
    const items = await pawEngine.taskActivity(undefined, 15);
    renderActivityList(container, items);
  } catch (e) {
    console.warn('[today] activity fetch error:', e);
    container.innerHTML = `<span class="today-loading">Unable to load activity</span>`;
  }
}

function renderActivityList(container: HTMLElement, items: EngineTaskActivity[]) {
  if (items.length === 0) {
    container.innerHTML = `<div class="today-section-empty">No recent activity</div>`;
    return;
  }

  container.innerHTML = items
    .map((item) => {
      const iconName = activityIcon(item.kind);
      const time = relativeTime(item.created_at);
      const content = truncateContent(item.content, 80);
      const agentTag = item.agent
        ? `<span class="activity-agent">${escHtml(item.agent)}</span>`
        : '';
      return `<div class="activity-item">
        <span class="activity-icon"><span class="ms ms-sm">${iconName}</span></span>
        <div class="activity-body">
          <span class="activity-content">${escHtml(content)}</span>
          ${agentTag}
        </div>
        <span class="activity-time">${time}</span>
      </div>`;
    })
    .join('');
}
