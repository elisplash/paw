// Memory Palace — Graph (canvas knowledge graph visualization)

import { pawEngine } from '../../engine';
import { $ } from '../../components/helpers';
import { CATEGORY_COLORS, type RecallCardData } from './atoms';

// ── Graph init ─────────────────────────────────────────────────────────────

let _graphBound = false;
export function initPalaceGraph(): void {
  if (_graphBound) return;
  _graphBound = true;
  const renderBtn = $('palace-graph-render');
  if (!renderBtn) return;

  renderBtn.addEventListener('click', () => renderPalaceGraph());
}

// ── Canvas knowledge graph ─────────────────────────────────────────────────

export async function renderPalaceGraph(): Promise<void> {
  const canvas = $('palace-graph-canvas') as HTMLCanvasElement | null;
  const emptyEl = $('palace-graph-empty');
  if (!canvas) return;

  if (emptyEl) {
    emptyEl.style.display = 'flex';
    emptyEl.textContent = 'Loading memory map…';
  }

  let memories: RecallCardData[] = [];

  try {
    const engineMems = await pawEngine.memoryList(50);
    memories = engineMems.map((m) => ({
      id: m.id,
      text: m.content,
      category: m.category,
      importance: m.importance,
      score: m.score,
    }));
  } catch (e) {
    console.warn('Graph load failed:', e);
    if (emptyEl) {
      emptyEl.style.display = 'flex';
      emptyEl.textContent = 'Failed to load memory map.';
    }
    return;
  }

  if (!memories.length) {
    if (emptyEl) {
      emptyEl.style.display = 'flex';
      emptyEl.textContent = 'No memories to visualize.';
    }
    return;
  }

  if (emptyEl) emptyEl.style.display = 'none';

  // Render bubble chart grouped by category
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const rect = canvas.parentElement?.getBoundingClientRect();
  canvas.width = rect?.width ?? 600;
  canvas.height = rect?.height ?? 400;

  // Group by category, place category clusters
  const groups = new Map<string, RecallCardData[]>();
  for (const mem of memories) {
    const cat = mem.category ?? 'other';
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat)!.push(mem);
  }

  // Layout: distribute category centers in a circle
  const categories = Array.from(groups.entries());
  const cx = canvas.width / 2,
    cy = canvas.height / 2;
  const radius = Math.min(cx, cy) * 0.55;

  ctx.clearRect(0, 0, canvas.width, canvas.height);

  categories.forEach(([cat, mems], i) => {
    const angle = (i / categories.length) * Math.PI * 2 - Math.PI / 2;
    const groupX = cx + Math.cos(angle) * radius;
    const groupY = cy + Math.sin(angle) * radius;

    // Draw category label
    ctx.fillStyle = '#676879';
    ctx.font = 'bold 12px Figtree, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(cat.toUpperCase(), groupX, groupY - 30 - mems.length * 2);

    // Draw bubbles for each memory
    mems.forEach((mem, j) => {
      const innerAngle = (j / mems.length) * Math.PI * 2;
      const spread = Math.min(25 + mems.length * 4, 60);
      const mx = groupX + Math.cos(innerAngle) * spread * (0.3 + Math.random() * 0.7);
      const my = groupY + Math.sin(innerAngle) * spread * (0.3 + Math.random() * 0.7);
      const size = 4 + (mem.importance ?? 5) * 0.8;
      const color = CATEGORY_COLORS[cat] ?? '#676879';

      ctx.beginPath();
      ctx.arc(mx, my, size, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.7;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    });

    // Count label
    ctx.fillStyle = CATEGORY_COLORS[cat] ?? '#676879';
    ctx.font = '11px Figtree, sans-serif';
    ctx.fillText(`${mems.length}`, groupX, groupY + 35 + mems.length * 2);
  });
}
