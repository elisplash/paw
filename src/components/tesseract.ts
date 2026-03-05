// ─────────────────────────────────────────────────────────────────────────────
// OpenPawz — Tesseract Indicator Component
// Ported from the OpenPawz website TesseractViewport renderer.
// 4D wireframe hypercube with glow + core dual-pass edges and vertex dots.
// States control rotation speed, colour, and glow intensity.
// ─────────────────────────────────────────────────────────────────────────────

/** Indicator state drives rotation speed, glow intensity, and colour cycling. */
export type TesseractState = 'idle' | 'thinking' | 'streaming' | 'done';

export interface TesseractOptions {
  /** Canvas side-length in CSS pixels. Default 24. */
  size?: number;
  /** Initial state. Default 'idle'. */
  state?: TesseractState;
  /** Override base colour (CSS colour string). Uses kinetic palette by default. */
  color?: string;
}

export interface TesseractInstance {
  /** Reference to the host canvas element. */
  canvas: HTMLCanvasElement;
  /** Change the animation state at runtime. */
  setState(s: TesseractState): void;
  /** Tear down the animation loop + remove the canvas. */
  destroy(): void;
}

// ── 4-D geometry (matches website TesseractViewport) ────────────────────────

/** 16 vertices of a unit tesseract at ±1 in each axis */
const VERTS: number[][] = [];
for (let i = 0; i < 16; i++) {
  VERTS.push([i & 1 ? 1 : -1, i & 2 ? 1 : -1, i & 4 ? 1 : -1, i & 8 ? 1 : -1]);
}

/** 32 edges — vertex pairs differing in exactly one bit */
const EDGES: [number, number][] = [];
for (let a = 0; a < 16; a++) {
  for (let b = a + 1; b < 16; b++) {
    const xor = a ^ b;
    if (xor && (xor & (xor - 1)) === 0) EDGES.push([a, b]);
  }
}

// ── 4D rotation (website-style: XW, YZ, ZW planes) ─────────────────────────

function rot4D(v: number[], xw: number, yz: number, zw: number): number[] {
  let [x, y, z, w] = v;
  // XW rotation
  const c1 = Math.cos(xw),
    s1 = Math.sin(xw);
  const nx = x * c1 - w * s1,
    nw = x * s1 + w * c1;
  x = nx;
  w = nw;
  // YZ rotation
  const c2 = Math.cos(yz),
    s2 = Math.sin(yz);
  const ny = y * c2 - z * s2,
    nz = y * s2 + z * c2;
  y = ny;
  z = nz;
  // ZW rotation
  const c3 = Math.cos(zw),
    s3 = Math.sin(zw);
  const nz2 = z * c3 - w * s3,
    nw2 = z * s3 + w * c3;
  z = nz2;
  w = nw2;
  return [x, y, z, w];
}

// ── Colour palettes per state (RGB, matching website kinetic colours) ───────

const STATE_COLORS: Record<TesseractState, { r: number; g: number; b: number }> = {
  idle: { r: 99, g: 102, b: 241 }, // indigo/accent
  thinking: { r: 168, g: 85, b: 247 }, // purple
  streaming: { r: 255, g: 77, b: 77 }, // kinetic red (atmo)
  done: { r: 143, g: 176, b: 160 }, // kinetic sage
};

/** Speed multipliers per state */
const STATE_SPEEDS: Record<TesseractState, number> = {
  idle: 0.3,
  thinking: 1.0,
  streaming: 2.2,
  done: 0.1,
};

/** Glow intensity per state (0–1) */
const STATE_GLOW: Record<TesseractState, number> = {
  idle: 0.4,
  thinking: 0.7,
  streaming: 1.0,
  done: 0.1,
};

// ── Public API ──────────────────────────────────────────────────────────────

/**
 * Create a tesseract indicator and attach it to the given container.
 * Rendering matches the website TesseractViewport: glow pass, core pass, vertex dots.
 */
export function createTesseract(
  container: HTMLElement,
  opts: TesseractOptions = {},
): TesseractInstance {
  const size = opts.size ?? 24;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);

  const canvas = document.createElement('canvas');
  canvas.className = 'tesseract-indicator';
  canvas.width = size * dpr;
  canvas.height = size * dpr;
  canvas.style.width = `${size}px`;
  canvas.style.height = `${size}px`;
  container.appendChild(canvas);

  const ctxRaw = canvas.getContext('2d');
  // In test environments (jsdom) canvas context may be null — static dot fallback
  if (!ctxRaw) {
    canvas.style.borderRadius = '50%';
    canvas.style.background = 'var(--accent, #6366f1)';
    return {
      canvas,
      setState() {
        /* no-op in fallback mode */
      },
      destroy() {
        canvas.remove();
      },
    };
  }
  const ctx = ctxRaw;

  let state: TesseractState = opts.state ?? 'idle';
  let destroyed = false;
  let frameId = 0;
  let t = 0;
  let currentSpeed = STATE_SPEEDS[state];
  let currentGlow = STATE_GLOW[state];

  const w = size * dpr;
  const h = size * dpr;
  const cx = w / 2;
  const cy = h / 2;

  // Scale factor — how big the tesseract appears relative to canvas
  const fl = Math.min(w, h) * 0.4;
  const d4 = 3.0; // 4D projection distance (matches website)

  let lastTs = 0;

  function frame(ts: number) {
    if (destroyed) return;

    const dt = lastTs ? (ts - lastTs) / 1000 : 0.016;
    lastTs = ts;

    // Smooth-lerp speed & glow towards target
    currentSpeed += (STATE_SPEEDS[state] - currentSpeed) * Math.min(1, dt * 4);
    currentGlow += (STATE_GLOW[state] - currentGlow) * Math.min(1, dt * 3);

    t += dt * currentSpeed;

    // ── 4D rotation angles (website-style) ──
    const xw = t * 0.8 + Math.sin(t * 0.15) * 0.8;
    const yz = t * 0.5 + Math.sin(t * 0.12 + 1.2) * 0.6;
    const zw = t * 0.3;

    // ── Colour ──
    const col = STATE_COLORS[state];
    const r = col.r,
      g = col.g,
      b = col.b;

    // ── Project all 16 vertices ──
    const sv: ([number, number] | null)[] = [];
    for (let i = 0; i < 16; i++) {
      const rv = rot4D(VERTS[i], xw, yz, zw);
      const den = d4 - rv[3] || 0.001;
      const s = d4 / den;
      const x3 = rv[0] * s;
      const y3 = rv[1] * s;
      const z3 = rv[2] * s;
      // Scale based on canvas size
      const sc = size * 0.16 * dpr;
      const x = x3 * sc;
      const y = y3 * sc;
      const z = z3 * sc;
      // Depth projection
      const depth = z + 5 * dpr;
      if (depth < 0.5) {
        sv.push(null);
        continue;
      }
      const ps = fl / depth;
      sv.push([cx + x * ps, cy - y * ps]);
    }

    // ── Clear ──
    ctx.clearRect(0, 0, w, h);

    // ── PASS 1: Glow (thick lines + shadowBlur) ──
    ctx.save();
    ctx.strokeStyle = `rgba(${r},${g},${b},${0.5 * currentGlow})`;
    ctx.lineWidth = 3 * dpr * (size < 16 ? 0.5 : size < 24 ? 0.7 : 1);
    ctx.shadowColor = `rgb(${r},${g},${b})`;
    ctx.shadowBlur = 12 * dpr * currentGlow;
    ctx.lineCap = 'round';
    ctx.beginPath();
    for (const [ai, bi] of EDGES) {
      const a = sv[ai],
        bv = sv[bi];
      if (a && bv) {
        ctx.moveTo(a[0], a[1]);
        ctx.lineTo(bv[0], bv[1]);
      }
    }
    ctx.stroke();
    ctx.restore();

    // ── PASS 2: Bright core (thin lines) ──
    ctx.save();
    const br = Math.min(255, r + 60),
      bg = Math.min(255, g + 60),
      bb = Math.min(255, b + 60);
    ctx.strokeStyle = `rgba(${br},${bg},${bb},0.85)`;
    ctx.lineWidth = 1.5 * dpr * (size < 16 ? 0.4 : size < 24 ? 0.6 : 1);
    ctx.shadowColor = `rgb(${r},${g},${b})`;
    ctx.shadowBlur = 4 * dpr * currentGlow;
    ctx.lineCap = 'round';
    ctx.beginPath();
    for (const [ai, bi] of EDGES) {
      const a = sv[ai],
        bv = sv[bi];
      if (a && bv) {
        ctx.moveTo(a[0], a[1]);
        ctx.lineTo(bv[0], bv[1]);
      }
    }
    ctx.stroke();
    ctx.restore();

    // ── PASS 3: Vertex dots (only for sizes ≥ 16) ──
    if (size >= 16) {
      ctx.save();
      const vr = Math.min(255, r + 80),
        vg = Math.min(255, g + 80),
        vb = Math.min(255, b + 80);
      ctx.fillStyle = `rgb(${vr},${vg},${vb})`;
      ctx.shadowColor = `rgb(${r},${g},${b})`;
      ctx.shadowBlur = 8 * dpr * currentGlow;
      const dotR = Math.max(1, 1.5 * dpr * (size < 24 ? 0.5 : 1));
      for (const p of sv) {
        if (p) {
          ctx.beginPath();
          ctx.arc(p[0], p[1], dotR, 0, Math.PI * 2);
          ctx.fill();
        }
      }
      ctx.restore();
    }

    // ── Done state: fade-out and stop ──
    if (state === 'done' && t > 3) {
      ctx.clearRect(0, 0, w, h);
      return;
    }

    frameId = requestAnimationFrame(frame);
  }

  frameId = requestAnimationFrame(frame);

  return {
    canvas,

    setState(s: TesseractState) {
      if (s === state) return;
      state = s;
      if (s !== 'done' && !frameId) {
        lastTs = 0;
        t = 0;
        frameId = requestAnimationFrame(frame);
      }
      if (s === 'done') t = 0;
    },

    destroy() {
      destroyed = true;
      cancelAnimationFrame(frameId);
      frameId = 0;
      canvas.remove();
    },
  };
}

// ── Inline HTML helper ──────────────────────────────────────────────────────

/**
 * Returns an HTML string placeholder for a tesseract indicator.
 * After inserting the HTML into the DOM, call `activateTesseracts(container)`
 * to hydrate all placeholders into live canvases.
 *
 * @example
 * el.innerHTML = `<div>${tesseractPlaceholder(20, 'thinking')}</div>`;
 * activateTesseracts(el);
 */
export function tesseractPlaceholder(size = 24, state: TesseractState = 'idle'): string {
  return `<span class="tesseract-mount" data-tesseract-size="${size}" data-tesseract-state="${state}"></span>`;
}

/** Registry of active tesseract instances for cleanup */
const _activeInstances = new WeakMap<HTMLElement, TesseractInstance>();

/**
 * Hydrate all `<span class="tesseract-mount">` placeholders inside the
 * given container into live tesseract canvases.
 */
export function activateTesseracts(root: HTMLElement): void {
  const mounts = root.querySelectorAll<HTMLElement>('.tesseract-mount');
  mounts.forEach((mount) => {
    // Skip if already activated
    if (_activeInstances.has(mount)) return;
    const size = parseInt(mount.dataset.tesseractSize || '24', 10);
    const state = (mount.dataset.tesseractState || 'idle') as TesseractState;
    const inst = createTesseract(mount, { size, state });
    _activeInstances.set(mount, inst);
  });
}

/**
 * Update the state of all active tesseract instances within a container.
 */
export function setTesseractState(root: HTMLElement, state: TesseractState): void {
  const mounts = root.querySelectorAll<HTMLElement>('.tesseract-mount');
  mounts.forEach((mount) => {
    const inst = _activeInstances.get(mount);
    if (inst) inst.setState(state);
  });
}

/**
 * Destroy all tesseract instances within a container.
 */
export function cleanupTesseracts(root: HTMLElement): void {
  const mounts = root.querySelectorAll<HTMLElement>('.tesseract-mount');
  mounts.forEach((mount) => {
    const inst = _activeInstances.get(mount);
    if (inst) {
      inst.destroy();
      _activeInstances.delete(mount);
    }
  });
}
