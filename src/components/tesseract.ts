// ─────────────────────────────────────────────────────────────────────────────
// OpenPawz — Tesseract Indicator Component
// A reactive 4D wireframe tesseract (hypercube) projected to 2D canvas.
// Used as an activity/status indicator throughout the app.
// States control rotation speed, glow, and color oscillation.
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

// ── 4-D geometry ────────────────────────────────────────────────────────────

/** 16 vertices of a unit hypercube centred at the origin (±0.5 in each axis) */
const VERTS: number[][] = [];
for (let i = 0; i < 16; i++) {
  VERTS.push([i & 1 ? 0.5 : -0.5, i & 2 ? 0.5 : -0.5, i & 4 ? 0.5 : -0.5, i & 8 ? 0.5 : -0.5]);
}

/** 32 edges of the hypercube — pairs of vertex indices that differ in exactly one bit */
const EDGES: [number, number][] = [];
for (let a = 0; a < 16; a++) {
  for (let b = a + 1; b < 16; b++) {
    const xor = a ^ b;
    if (xor && (xor & (xor - 1)) === 0) EDGES.push([a, b]);
  }
}

// ── Rotation matrices (4-D planes) ─────────────────────────────────────────

function rotXY(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2], v[3]];
}
function rotXZ(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [c * v[0] - s * v[2], v[1], s * v[0] + c * v[2], v[3]];
}
function rotXW(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [c * v[0] - s * v[3], v[1], v[2], s * v[0] + c * v[3]];
}
function rotYZ(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2], v[3]];
}
function rotYW(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [v[0], c * v[1] - s * v[3], v[2], s * v[1] + c * v[3]];
}
function rotZW(v: number[], a: number): number[] {
  const c = Math.cos(a),
    s = Math.sin(a);
  return [v[0], v[1], c * v[2] - s * v[3], s * v[2] + c * v[3]];
}

// ── Projection helpers ──────────────────────────────────────────────────────

/** Stereographic-style 4D → 3D projection */
function project4to3(v: number[], d: number): [number, number, number] {
  const w = 1 / (d - v[3]);
  return [v[0] * w, v[1] * w, v[2] * w];
}

/** Simple perspective 3D → 2D projection */
function project3to2(v: [number, number, number], d: number): [number, number] {
  const w = 1 / (d - v[2]);
  return [v[0] * w, v[1] * w];
}

// ── Colour palette ──────────────────────────────────────────────────────────

/** HSL colour cycling palettes per state */
const STATE_PALETTES: Record<
  TesseractState,
  { hueBase: number; hueRange: number; sat: number; lum: number }
> = {
  idle: { hueBase: 220, hueRange: 60, sat: 55, lum: 65 }, // cool blue-purple drift
  thinking: { hueBase: 270, hueRange: 90, sat: 70, lum: 60 }, // purple-magenta cycle
  streaming: { hueBase: 340, hueRange: 120, sat: 85, lum: 58 }, // hot pink-orange-red
  done: { hueBase: 150, hueRange: 30, sat: 60, lum: 60 }, // sage green settle
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
  idle: 0.15,
  thinking: 0.4,
  streaming: 0.85,
  done: 0.0,
};

// ── Public API ──────────────────────────────────────────────────────────────

/**
 * Create a tesseract indicator and attach it to the given container.
 * The returned controller lets you change state and tear it down.
 *
 * ```ts
 * const t = createTesseract(myDiv, { size: 32, state: 'thinking' });
 * t.setState('streaming');
 * // later …
 * t.destroy();
 * ```
 */
export function createTesseract(
  container: HTMLElement,
  opts: TesseractOptions = {},
): TesseractInstance {
  const size = opts.size ?? 24;
  const dpr = window.devicePixelRatio || 1;

  const canvas = document.createElement('canvas');
  canvas.className = 'tesseract-indicator';
  canvas.width = size * dpr;
  canvas.height = size * dpr;
  canvas.style.width = `${size}px`;
  canvas.style.height = `${size}px`;
  container.appendChild(canvas);

  const ctxRaw = canvas.getContext('2d');
  // In test environments (jsdom) canvas context may be null — render a static dot fallback
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
  ctx.scale(dpr, dpr);

  let state: TesseractState = opts.state ?? 'idle';
  const overrideColor = opts.color;
  let destroyed = false;
  let frameId = 0;
  let t = 0; // accumulated time
  let currentSpeed = STATE_SPEEDS[state]; // lerped
  let currentGlow = STATE_GLOW[state]; // lerped

  // ── Render loop ───────────────────────────────────────────────────

  let lastTs = 0;

  function frame(ts: number) {
    if (destroyed) return;

    const dt = lastTs ? (ts - lastTs) / 1000 : 0.016;
    lastTs = ts;

    // Smooth-lerp speed & glow towards target
    const targetSpeed = STATE_SPEEDS[state];
    const targetGlow = STATE_GLOW[state];
    currentSpeed += (targetSpeed - currentSpeed) * Math.min(1, dt * 4);
    currentGlow += (targetGlow - currentGlow) * Math.min(1, dt * 3);

    t += dt * currentSpeed;

    // ── Angles (6 rotation planes, each at different rates) ──
    const a1 = t * 0.7;
    const a2 = t * 0.5;
    const a3 = t * 0.3;
    const a4 = t * 0.9;
    const a5 = t * 0.4;
    const a6 = t * 0.6;

    // ── Project vertices ──
    const pts2d: [number, number][] = [];
    // At small sizes, use a tighter projection for better visibility
    const projScale = size < 16 ? size * 0.42 : size * 0.38;
    for (const v of VERTS) {
      let r = v;
      r = rotXY(r, a1);
      r = rotXZ(r, a2);
      r = rotXW(r, a3);
      r = rotYZ(r, a4);
      // Skip some rotations at tiny sizes for visual clarity
      if (size >= 16) {
        r = rotYW(r, a5);
        r = rotZW(r, a6);
      }
      const p3 = project4to3(r, 2.0);
      const p2 = project3to2(p3, 2.5);
      // Scale to canvas (centre + scale)
      pts2d.push([size / 2 + p2[0] * projScale, size / 2 + p2[1] * projScale]);
    }

    // ── Colour ──
    const palette = STATE_PALETTES[state];
    const hue = palette.hueBase + Math.sin(t * 1.5) * palette.hueRange;
    const strokeColor = overrideColor ?? `hsl(${hue}, ${palette.sat}%, ${palette.lum}%)`;
    const glowColor = overrideColor ?? `hsl(${hue}, ${palette.sat}%, ${palette.lum}%)`;

    // ── Clear ──
    ctx.clearRect(0, 0, size, size);

    // ── Glow layer ──
    if (currentGlow > 0.02) {
      ctx.save();
      ctx.globalAlpha = currentGlow * 0.6;
      ctx.shadowColor = glowColor;
      ctx.shadowBlur = size * 0.25;
      ctx.strokeStyle = glowColor;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (const [a, b] of EDGES) {
        ctx.moveTo(pts2d[a][0], pts2d[a][1]);
        ctx.lineTo(pts2d[b][0], pts2d[b][1]);
      }
      ctx.stroke();
      ctx.restore();
    }

    // ── Main wireframe ──
    ctx.save();
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = size >= 24 ? 1.0 : size >= 16 ? 0.8 : 1.2;
    ctx.globalAlpha = size < 16 ? 1.0 : 0.85;
    ctx.beginPath();
    for (const [a, b] of EDGES) {
      ctx.moveTo(pts2d[a][0], pts2d[a][1]);
      ctx.lineTo(pts2d[b][0], pts2d[b][1]);
    }
    ctx.stroke();
    ctx.restore();

    // ── Done state: scale-down animation ──
    if (state === 'done') {
      const scaleDown = Math.max(0, 1 - t * 0.3);
      if (scaleDown <= 0.01) {
        ctx.clearRect(0, 0, size, size);
        // Don't request another frame — just sit cleared
        return;
      }
    }

    frameId = requestAnimationFrame(frame);
  }

  frameId = requestAnimationFrame(frame);

  return {
    canvas,

    setState(s: TesseractState) {
      if (s === state) return;
      state = s;
      // If restarting from 'done', reset time and ensure loop is running
      if (s !== 'done') {
        if (!frameId) {
          lastTs = 0;
          t = 0;
          frameId = requestAnimationFrame(frame);
        }
      }
      if (s === 'done') {
        t = 0; // reset for the collapse animation
      }
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
