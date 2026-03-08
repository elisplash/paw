// src/components/molecules/data-viz.ts
// Reusable data-visualization atoms — sparklines, heatmaps, progress bars, status dots.
// All render pure HTML/SVG strings. No framework dependency.

/**
 * Inline SVG sparkline chart.
 * Renders a smooth polyline from an array of numbers.
 */
export function sparkline(
  data: number[],
  color = 'var(--accent)',
  width = 80,
  height = 24,
): string {
  if (!data.length) return '';
  const max = Math.max(...data, 1);
  const min = Math.min(...data, 0);
  const range = max - min || 1;
  const step = width / Math.max(data.length - 1, 1);

  const points = data
    .map((v, i) => {
      const x = (i * step).toFixed(1);
      const y = (height - ((v - min) / range) * height).toFixed(1);
      return `${x},${y}`;
    })
    .join(' ');

  return `<svg class="viz-sparkline" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
    <polyline points="${points}" stroke="${color}" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
  </svg>`;
}

/**
 * GitHub-style 30-day activity heatmap strip.
 * Each day maps to a small colored square; intensity reflects count.
 */
export function heatmapStrip(
  days: { date: string; count: number }[],
  color = 'var(--accent)',
): string {
  const max = Math.max(...days.map((d) => d.count), 1);

  const cells = days
    .map((d) => {
      const opacity = d.count === 0 ? 0 : 0.2 + (d.count / max) * 0.8;
      const bg = d.count === 0 ? 'var(--bg-tertiary)' : color;
      return `<div class="viz-heatmap-cell" title="${d.date}: ${d.count}" style="background:${bg};opacity:${opacity.toFixed(2)}"></div>`;
    })
    .join('');

  const legendCells = [0.15, 0.35, 0.55, 0.75, 1]
    .map(
      (op) =>
        `<div class="viz-heatmap-cell viz-heatmap-legend-cell" style="background:${color};opacity:${op}"></div>`,
    )
    .join('');

  return `<div class="viz-heatmap">${cells}</div>
    <div class="viz-heatmap-legend">
      <span>less</span>
      <div class="viz-heatmap-legend-cells">${legendCells}</div>
      <span>more</span>
    </div>`;
}

/**
 * Horizontal progress bar with optional label.
 */
export function progressBar(percent: number, color = 'var(--accent)', label?: string): string {
  const clamped = Math.max(0, Math.min(100, percent));
  const labelHtml = label
    ? `<div style="display:flex;justify-content:space-between;margin-bottom:4px">
        <span style="font-size:var(--type-label);color:var(--text-muted)">${label}</span>
        <span style="font-size:var(--type-label);color:var(--text-secondary)">${Math.round(clamped)}%</span>
       </div>`
    : '';

  return `${labelHtml}<div class="viz-progress">
    <div class="viz-progress-fill" style="width:${clamped}%;background:${color}"></div>
  </div>`;
}

/**
 * Circular SVG ring gauge.
 */
export function progressRing(percent: number, color = 'var(--accent)', size = 48): string {
  const clamped = Math.max(0, Math.min(100, percent));
  const strokeWidth = 4;
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (clamped / 100) * circumference;

  return `<svg class="viz-sparkline" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
    <circle cx="${size / 2}" cy="${size / 2}" r="${radius}"
      stroke="var(--bg-tertiary)" stroke-width="${strokeWidth}" fill="none"/>
    <circle cx="${size / 2}" cy="${size / 2}" r="${radius}"
      stroke="${color}" stroke-width="${strokeWidth}" fill="none"
      stroke-dasharray="${circumference}" stroke-dashoffset="${offset}"
      stroke-linecap="round"
      transform="rotate(-90 ${size / 2} ${size / 2})"
      style="transition:stroke-dashoffset 0.6s ease-out"/>
  </svg>`;
}

/**
 * Status indicator dot with optional pulse animation.
 */
export function statusDot(state: 'idle' | 'active' | 'error' | 'offline'): string {
  const cls = `viz-dot viz-dot-${state}`;
  return `<span class="${cls}"></span>`;
}

/**
 * Animate a number from 0 to target, updating element textContent.
 * Uses requestAnimationFrame for smooth count-up.
 */
export function animateCountUp(element: HTMLElement, target: number, duration = 600): void {
  const start = performance.now();
  const isFloat = target % 1 !== 0;

  function tick(now: number) {
    const elapsed = now - start;
    const progress = Math.min(elapsed / duration, 1);
    // ease-out cubic
    const eased = 1 - Math.pow(1 - progress, 3);
    const current = eased * target;

    element.textContent = isFloat ? current.toFixed(2) : Math.round(current).toString();

    if (progress < 1) {
      requestAnimationFrame(tick);
    }
  }

  requestAnimationFrame(tick);
}
