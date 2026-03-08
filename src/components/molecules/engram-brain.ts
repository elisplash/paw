// src/components/molecules/engram-brain.ts
// Animated Engram brain canvas — neuron nodes with electron particle flow.
// Pure canvas, no dependencies. Mirrors the kinetic accent palette.

export interface EngramBrainInstance {
  destroy(): void;
}

// ── Neuron node layout (relative coords, 0–1) ─────────────────────────────
const NODES = [
  { x: 0.37, y: 0.22 }, // Front-left
  { x: 0.63, y: 0.22 }, // Front-right
  { x: 0.2, y: 0.43 }, // Mid-left outer
  { x: 0.8, y: 0.43 }, // Mid-right outer
  { x: 0.3, y: 0.62 }, // Lower-left
  { x: 0.7, y: 0.62 }, // Lower-right
  { x: 0.41, y: 0.78 }, // Back-left
  { x: 0.59, y: 0.78 }, // Back-right
  { x: 0.5, y: 0.3 }, // Top-center (corpus callosum)
  { x: 0.5, y: 0.55 }, // Center
  { x: 0.33, y: 0.4 }, // Left mid
  { x: 0.67, y: 0.4 }, // Right mid
];

// Static connection pairs for the dim lattice drawn under electrons
const CONNECTIONS: [number, number][] = [
  [0, 8],
  [1, 8],
  [8, 9],
  [2, 10],
  [3, 11],
  [4, 9],
  [5, 9],
  [6, 7],
  [10, 11],
  [0, 10],
  [1, 11],
  [4, 6],
  [5, 7],
];

interface Electron {
  from: number;
  to: number;
  t: number; // 0–1 journey progress
  speed: number;
  trail: { x: number; y: number }[];
  cpOffset: number; // bezier control-point perpendicular offset
}

// ── Quadratic bezier evaluation ───────────────────────────────────────────
function quadBezier(
  p0x: number,
  p0y: number,
  p1x: number,
  p1y: number,
  cpx: number,
  cpy: number,
  t: number,
): { x: number; y: number } {
  const mt = 1 - t;
  return {
    x: mt * mt * p0x + 2 * mt * t * cpx + t * t * p1x,
    y: mt * mt * p0y + 2 * mt * t * cpy + t * t * p1y,
  };
}

function controlPoint(
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
  offset: number,
): { cpx: number; cpy: number } {
  const mx = (fromX + toX) / 2;
  const my = (fromY + toY) / 2;
  const dx = toX - fromX;
  const dy = toY - fromY;
  const len = Math.sqrt(dx * dx + dy * dy) || 1;
  return { cpx: mx - (dy / len) * offset, cpy: my + (dx / len) * offset };
}

export function createEngramBrain(container: HTMLElement): EngramBrainInstance {
  const canvas = document.createElement('canvas');
  canvas.className = 'engram-brain-canvas';
  container.style.position = 'relative';
  container.appendChild(canvas);

  let w = 0;
  let h = 0;
  let animId = 0;
  let dpr = 1;

  // Electrons — distributed so they start at different waypoints
  const electrons: Electron[] = Array.from({ length: 12 }, (_, i) => ({
    from: i % NODES.length,
    to: (i + 4) % NODES.length,
    t: i / 12 + Math.random() * 0.05,
    speed: 0.002 + Math.random() * 0.0025,
    trail: [],
    cpOffset: (Math.random() - 0.5) * 70,
  }));

  // ── Resize — honours devicePixelRatio for sharp rendering ────────────────
  function resize() {
    const rect = container.getBoundingClientRect();
    dpr = window.devicePixelRatio || 1;
    w = Math.max(rect.width, 1);
    h = Math.max(rect.height, 1);
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  }

  // ── Draw brain outline ────────────────────────────────────────────────────
  function drawBrain(ctx: CanvasRenderingContext2D) {
    const cx = w * 0.5;
    const cy = h * 0.5;
    const rx = Math.min(w, h) * 0.38;
    const ry = Math.min(w, h) * 0.34;

    ctx.save();

    // Ambient inner glow
    const glow = ctx.createRadialGradient(cx, cy, ry * 0.1, cx, cy, rx * 1.15);
    glow.addColorStop(0, 'rgba(212,101,74,0.07)');
    glow.addColorStop(1, 'rgba(212,101,74,0)');
    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.ellipse(cx, cy, rx * 1.15, ry * 1.15, 0, 0, Math.PI * 2);
    ctx.fill();

    // Outer ellipse
    ctx.beginPath();
    ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
    ctx.strokeStyle = 'rgba(212,101,74,0.22)';
    ctx.lineWidth = 1.5;
    ctx.stroke();

    // Longitudinal fissure
    ctx.beginPath();
    ctx.moveTo(cx, cy - ry * 0.86);
    ctx.lineTo(cx, cy + ry * 0.5);
    ctx.strokeStyle = 'rgba(212,101,74,0.18)';
    ctx.lineWidth = 1;
    ctx.stroke();

    // Left-hemisphere sulci
    const sulciL: [number, number, number, number, number, number][] = [
      [
        cx - rx * 0.24,
        cy - ry * 0.58,
        cx - rx * 0.54,
        cy - ry * 0.06,
        cx - rx * 0.4,
        cy + ry * 0.4,
      ],
      [
        cx - rx * 0.09,
        cy - ry * 0.66,
        cx - rx * 0.28,
        cy - ry * 0.01,
        cx - rx * 0.18,
        cy + ry * 0.5,
      ],
      [
        cx - rx * 0.48,
        cy - ry * 0.22,
        cx - rx * 0.65,
        cy + ry * 0.14,
        cx - rx * 0.48,
        cy + ry * 0.44,
      ],
    ];
    const sulciR: [number, number, number, number, number, number][] = sulciL.map(
      ([x0, y0, x1, y1, x2, y2]) => [2 * cx - x0, y0, 2 * cx - x1, y1, 2 * cx - x2, y2],
    );

    [...sulciL, ...sulciR].forEach(([x0, y0, x1, y1, x2, y2], i) => {
      ctx.beginPath();
      ctx.moveTo(x0, y0);
      ctx.quadraticCurveTo(x1, y1, x2, y2);
      ctx.strokeStyle = `rgba(212,101,74,${i < 3 ? 0.11 : 0.09})`;
      ctx.lineWidth = 1;
      ctx.stroke();
    });

    // Parietal-occipital cross-sulcus
    ctx.beginPath();
    ctx.moveTo(cx - rx * 0.16, cy + ry * 0.32);
    ctx.quadraticCurveTo(cx, cy + ry * 0.56, cx + rx * 0.16, cy + ry * 0.32);
    ctx.strokeStyle = 'rgba(212,101,74,0.09)';
    ctx.stroke();

    ctx.restore();
  }

  // ── Draw dim connection lattice ───────────────────────────────────────────
  function drawConnections(ctx: CanvasRenderingContext2D) {
    ctx.save();
    CONNECTIONS.forEach(([a, b]) => {
      const na = NODES[a];
      const nb = NODES[b];
      ctx.beginPath();
      ctx.moveTo(na.x * w, na.y * h);
      ctx.lineTo(nb.x * w, nb.y * h);
      ctx.strokeStyle = 'rgba(212,101,74,0.06)';
      ctx.lineWidth = 0.8;
      ctx.stroke();
    });
    ctx.restore();
  }

  // ── Draw pulsing neuron nodes ─────────────────────────────────────────────
  function drawNodes(ctx: CanvasRenderingContext2D, time: number) {
    ctx.save();
    NODES.forEach((n, i) => {
      const nx = n.x * w;
      const ny = n.y * h;
      const phase = time * 0.00075 + i * 0.65;
      const pulse = 0.5 + 0.5 * Math.sin(phase);

      // Glow halo
      const halo = ctx.createRadialGradient(nx, ny, 0, nx, ny, 11 + pulse * 5);
      halo.addColorStop(0, `rgba(212,101,74,${0.4 + pulse * 0.22})`);
      halo.addColorStop(1, 'rgba(212,101,74,0)');
      ctx.fillStyle = halo;
      ctx.beginPath();
      ctx.arc(nx, ny, 11 + pulse * 5, 0, Math.PI * 2);
      ctx.fill();

      // Core dot
      ctx.beginPath();
      ctx.arc(nx, ny, 2.4, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(255,160,110,${0.75 + pulse * 0.22})`;
      ctx.fill();
    });
    ctx.restore();
  }

  // ── Draw electrons with trails ───────────────────────────────────────────
  function drawElectrons(ctx: CanvasRenderingContext2D) {
    ctx.save();
    electrons.forEach((e) => {
      const from = NODES[e.from];
      const to = NODES[e.to];
      const fromX = from.x * w;
      const fromY = from.y * h;
      const toX = to.x * w;
      const toY = to.y * h;
      const { cpx, cpy } = controlPoint(fromX, fromY, toX, toY, e.cpOffset);
      const pos = quadBezier(fromX, fromY, toX, toY, cpx, cpy, e.t);

      // Trail
      if (e.trail.length > 1) {
        for (let i = 1; i < e.trail.length; i++) {
          const prev = e.trail[i - 1];
          const curr = e.trail[i];
          const alpha = (i / e.trail.length) * 0.55;
          ctx.beginPath();
          ctx.moveTo(prev.x, prev.y);
          ctx.lineTo(curr.x, curr.y);
          ctx.strokeStyle = `rgba(212,101,74,${alpha})`;
          ctx.lineWidth = 1.3;
          ctx.stroke();
        }
      }

      // Electron glow + core
      const grd = ctx.createRadialGradient(pos.x, pos.y, 0, pos.x, pos.y, 7);
      grd.addColorStop(0, 'rgba(255,160,110,0.9)');
      grd.addColorStop(1, 'rgba(212,101,74,0)');
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(pos.x, pos.y, 7, 0, Math.PI * 2);
      ctx.fill();

      ctx.beginPath();
      ctx.arc(pos.x, pos.y, 2, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(255,220,180,1)';
      ctx.fill();
    });
    ctx.restore();
  }

  // ── Main animation loop ───────────────────────────────────────────────────
  let lastTime = 0;

  function frame(time: number) {
    if (!lastTime) lastTime = time;
    const dt = Math.min(time - lastTime, 50);
    lastTime = time;

    const ctx = canvas.getContext('2d');
    if (!ctx) {
      animId = requestAnimationFrame(frame);
      return;
    }

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    // Advance electrons
    electrons.forEach((e) => {
      const from = NODES[e.from];
      const to = NODES[e.to];
      const fromX = from.x * w;
      const fromY = from.y * h;
      const toX = to.x * w;
      const toY = to.y * h;
      const { cpx, cpy } = controlPoint(fromX, fromY, toX, toY, e.cpOffset);
      const pos = quadBezier(fromX, fromY, toX, toY, cpx, cpy, e.t);

      e.trail.push({ x: pos.x, y: pos.y });
      if (e.trail.length > 16) e.trail.shift();

      e.t += e.speed * (dt / 16.67);
      if (e.t >= 1) {
        e.t = 0;
        e.trail = [];
        e.from = e.to;
        let next = Math.floor(Math.random() * NODES.length);
        while (next === e.from) next = Math.floor(Math.random() * NODES.length);
        e.to = next;
        e.cpOffset = (Math.random() - 0.5) * 70;
      }
    });

    drawBrain(ctx);
    drawConnections(ctx);
    drawNodes(ctx, time);
    drawElectrons(ctx);

    animId = requestAnimationFrame(frame);
  }

  resize();
  animId = requestAnimationFrame(frame);

  const ro = new ResizeObserver(resize);
  ro.observe(container);

  return {
    destroy() {
      cancelAnimationFrame(animId);
      ro.disconnect();
      canvas.remove();
    },
  };
}
