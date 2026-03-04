// Canvas Chart — Lightweight SVG chart renderer.
// No charting library — pure SVG generation following Pawz no-framework philosophy.
// Supports: line, bar, area, pie, sparkline.

import { escHtml } from '../helpers';

// ── Public API ────────────────────────────────────────────────────────

/** Render chart data to an SVG string based on chart_type. */
export function renderSvgChart(data: Record<string, unknown>): string {
  const chartType = String(data.chart_type ?? data.type ?? 'line');
  const series = (data.series as ChartSeries[]) ?? [];
  const labels = (data.labels as string[]) ?? [];

  switch (chartType) {
    case 'line':
      return renderLineChart(series, labels);
    case 'area':
      return renderAreaChart(series, labels);
    case 'bar':
      return renderBarChart(series, labels);
    case 'pie':
      return renderPieChart(series);
    case 'sparkline':
      return renderSparkline(series);
    default:
      return `<p class="canvas-muted">Unknown chart type: ${escHtml(chartType)}</p>`;
  }
}

// ── Types ─────────────────────────────────────────────────────────────

interface ChartSeries {
  name?: string;
  values?: number[];
  value?: number; // for pie slices
  color?: string;
  label?: string; // for pie slices
}

// ── Constants ─────────────────────────────────────────────────────────

const W = 300;
const H = 160;
const PAD = 24;
const COLORS = ['#6366f1', '#22d3ee', '#f59e0b', '#ef4444', '#10b981', '#8b5cf6'];

// ── Line Chart ────────────────────────────────────────────────────────

function renderLineChart(series: ChartSeries[], labels: string[]): string {
  if (!series.length) return noData();
  const { minV, maxV, scaleX, scaleY } = computeScales(series);

  const lines = series
    .map((s, si) => {
      const vals = s.values ?? [];
      const color = s.color ?? COLORS[si % COLORS.length];
      const points = vals.map((v, i) => `${scaleX(i, vals.length)},${scaleY(v, minV, maxV)}`);
      return `<polyline points="${points.join(' ')}" fill="none" stroke="${color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>`;
    })
    .join('');

  return wrapSvg(lines + renderXLabels(labels, series[0]?.values?.length ?? 0));
}

// ── Area Chart ────────────────────────────────────────────────────────

function renderAreaChart(series: ChartSeries[], labels: string[]): string {
  if (!series.length) return noData();
  const { minV, maxV, scaleX, scaleY } = computeScales(series);
  const bottom = H - PAD;

  const areas = series
    .map((s, si) => {
      const vals = s.values ?? [];
      const color = s.color ?? COLORS[si % COLORS.length];
      const points = vals.map((v, i) => `${scaleX(i, vals.length)},${scaleY(v, minV, maxV)}`);
      const first = scaleX(0, vals.length);
      const last = scaleX(vals.length - 1, vals.length);
      const polyPoints = `${first},${bottom} ${points.join(' ')} ${last},${bottom}`;
      return `<polygon points="${polyPoints}" fill="${color}" fill-opacity="0.2" stroke="${color}" stroke-width="1.5"/>`;
    })
    .join('');

  return wrapSvg(areas + renderXLabels(labels, series[0]?.values?.length ?? 0));
}

// ── Bar Chart ─────────────────────────────────────────────────────────

function renderBarChart(series: ChartSeries[], labels: string[]): string {
  const vals = series[0]?.values ?? [];
  if (!vals.length) return noData();

  const max = Math.max(...vals, 1);
  const barW = Math.max(4, (W - PAD * 2) / vals.length - 4);
  const chartH = H - PAD * 2;

  const bars = vals
    .map((v, i) => {
      const color = series[0]?.color ?? COLORS[i % COLORS.length];
      const h = (v / max) * chartH;
      const x = PAD + i * ((W - PAD * 2) / vals.length) + 2;
      const y = H - PAD - h;
      return `<rect x="${x}" y="${y}" width="${barW}" height="${h}" rx="2" fill="${color}" fill-opacity="0.8"/>`;
    })
    .join('');

  return wrapSvg(bars + renderXLabels(labels, vals.length));
}

// ── Pie Chart ─────────────────────────────────────────────────────────

function renderPieChart(series: ChartSeries[]): string {
  const slices = series.filter((s) => (s.value ?? 0) > 0);
  if (!slices.length) return noData();

  const total = slices.reduce((sum, s) => sum + (s.value ?? 0), 0);
  const cx = W / 2;
  const cy = H / 2;
  const r = Math.min(cx, cy) - PAD;

  let startAngle = -Math.PI / 2;
  const paths = slices
    .map((s, i) => {
      const val = s.value ?? 0;
      const angle = (val / total) * Math.PI * 2;
      const endAngle = startAngle + angle;
      const x1 = cx + r * Math.cos(startAngle);
      const y1 = cy + r * Math.sin(startAngle);
      const x2 = cx + r * Math.cos(endAngle);
      const y2 = cy + r * Math.sin(endAngle);
      const large = angle > Math.PI ? 1 : 0;
      const color = s.color ?? COLORS[i % COLORS.length];
      const d = `M ${cx} ${cy} L ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2} Z`;
      startAngle = endAngle;
      return `<path d="${d}" fill="${color}" fill-opacity="0.85" stroke="var(--bg-primary)" stroke-width="1"/>`;
    })
    .join('');

  // Legend
  const legend = slices
    .map((s, i) => {
      const color = s.color ?? COLORS[i % COLORS.length];
      const label = s.label ?? s.name ?? `Slice ${i + 1}`;
      return `<span class="canvas-chart-legend-item"><span class="canvas-chart-dot" style="background:${color}"></span>${escHtml(label)}</span>`;
    })
    .join('');

  return `${wrapSvg(paths)}<div class="canvas-chart-legend">${legend}</div>`;
}

// ── Sparkline ─────────────────────────────────────────────────────────

function renderSparkline(series: ChartSeries[]): string {
  const vals = series[0]?.values ?? [];
  if (vals.length < 2) return noData();

  const min = Math.min(...vals);
  const max = Math.max(...vals);
  const range = max - min || 1;
  const w = 120;
  const h = 32;

  const points = vals
    .map((v, i) => {
      const x = (i / (vals.length - 1)) * w;
      const y = h - ((v - min) / range) * (h - 4) - 2;
      return `${x},${y}`;
    })
    .join(' ');

  const color = series[0]?.color ?? COLORS[0];
  return `<svg viewBox="0 0 ${w} ${h}" class="canvas-sparkline"><polyline points="${points}" fill="none" stroke="${color}" stroke-width="1.5" stroke-linecap="round"/></svg>`;
}

// ── Helpers ───────────────────────────────────────────────────────────

function computeScales(series: ChartSeries[]) {
  const allVals = series.flatMap((s) => s.values ?? []);
  const minV = Math.min(...allVals);
  const maxV = Math.max(...allVals);
  const range = maxV - minV || 1;

  const scaleX = (i: number, len: number) => PAD + (i / Math.max(len - 1, 1)) * (W - PAD * 2);
  const scaleY = (v: number, min: number, max: number) =>
    H - PAD - ((v - min) / (max - min || 1)) * (H - PAD * 2);

  return { minV, maxV, range, scaleX, scaleY };
}

function renderXLabels(labels: string[], count: number): string {
  if (!labels.length || !count) return '';
  // Show at most 6 labels to avoid overlap
  const step = Math.ceil(count / 6);
  return labels
    .filter((_, i) => i % step === 0)
    .map((l, i) => {
      const x = PAD + ((i * step) / Math.max(count - 1, 1)) * (W - PAD * 2);
      return `<text x="${x}" y="${H - 4}" text-anchor="middle" class="canvas-chart-label">${escHtml(l)}</text>`;
    })
    .join('');
}

function wrapSvg(content: string): string {
  return `<svg viewBox="0 0 ${W} ${H}" class="canvas-chart">${content}</svg>`;
}

function noData(): string {
  return '<p class="canvas-muted">No chart data</p>';
}
