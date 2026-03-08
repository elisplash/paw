// ─────────────────────────────────────────────────────────────────────────────
// OpenPawz — Hero Logo + Status Ring
// Replaces the hero tesseract on the Today dashboard.
// Renders the OpenPawz logo centered with:
//   • Animated orbital status ring (segments for system components)
//   • Breathing glow pulse matching the accent colour
//   • Interactive pointer-driven parallax on the logo
//   • Reactive segment states (active/inactive/error)
// ─────────────────────────────────────────────────────────────────────────────

/** Segment state drives opacity and pulse intensity */
export type SegmentState = 'active' | 'inactive' | 'error';

export interface HeroLogoInstance {
  canvas: HTMLCanvasElement;
  destroy(): void;
  resize(): void;
  /** Update the state of a segment by label (CONDUCTOR, ENGRAM, LIBRARIAN, FOREMAN) */
  setSegmentState(label: string, state: SegmentState): void;
}

// ── Accent colour (consistent, not multicolour) ────────────────────────────

const ACCENT: [number, number, number] = [99, 102, 241]; // indigo/accent
const ERROR_COLOR: [number, number, number] = [255, 77, 77]; // red for errors

// ── Status ring segments (system components) ────────────────────────────────

interface RingSegment {
  label: string;
  /** Arc length as fraction of full circle (sums to ~0.85 to leave gaps) */
  arc: number;
  state: SegmentState;
}

const INITIAL_SEGMENTS: Omit<RingSegment, 'state'>[] = [
  { label: 'CONDUCTOR', arc: 0.22 },
  { label: 'ENGRAM', arc: 0.18 },
  { label: 'LIBRARIAN', arc: 0.2 },
  { label: 'FOREMAN', arc: 0.22 },
];

const GAP = 0.04; // gap between segments (radians fraction of 2π)

/**
 * Create a hero logo canvas that fills its container.
 * Renders the pawz logo at centre with animated orbital status ring.
 * Responds to mouse/touch for subtle parallax.
 */
export function createHeroLogo(container: HTMLElement): HeroLogoInstance {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);

  const canvas = document.createElement('canvas');
  canvas.className = 'hero-logo-canvas';
  canvas.style.display = 'block';
  canvas.style.width = '100%';
  canvas.style.height = '100%';
  container.appendChild(canvas);

  const ctxRaw = canvas.getContext('2d');
  if (!ctxRaw) {
    return {
      canvas,
      destroy() {
        canvas.remove();
      },
      resize() {},
      setSegmentState() {},
    };
  }
  const ctx = ctxRaw;

  let destroyed = false;
  let frameId = 0;
  const t0 = performance.now();

  // ── Reactive segment state ──
  const segments: RingSegment[] = INITIAL_SEGMENTS.map((s) => ({
    ...s,
    state: 'active' as SegmentState,
  }));

  // ── Load logo image ──
  const logo = new Image();
  logo.src = '/images/pawz-logo-transparent.png';
  let logoReady = false;
  logo.onload = () => {
    logoReady = true;
  };

  // ── Pointer state (normalised -1 to 1) ──
  let pointerX = 0;
  let pointerY = 0;
  let targetPX = 0;
  let targetPY = 0;

  function onPointerMove(e: PointerEvent | MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    targetPX = ((e.clientX - rect.left) / rect.width - 0.5) * 2;
    targetPY = ((e.clientY - rect.top) / rect.height - 0.5) * 2;
  }
  function onPointerLeave() {
    targetPX = 0;
    targetPY = 0;
  }
  function onTouchMove(e: TouchEvent) {
    if (!e.touches.length) return;
    const touch = e.touches[0];
    const rect = canvas.getBoundingClientRect();
    targetPX = ((touch.clientX - rect.left) / rect.width - 0.5) * 2;
    targetPY = ((touch.clientY - rect.top) / rect.height - 0.5) * 2;
  }

  canvas.addEventListener('pointermove', onPointerMove);
  canvas.addEventListener('pointerleave', onPointerLeave);
  canvas.addEventListener('touchmove', onTouchMove, { passive: true });
  canvas.style.touchAction = 'none';

  // ── Sizing ──
  function syncSize() {
    const rect = container.getBoundingClientRect();
    const w = Math.round(rect.width * dpr);
    const h = Math.round(rect.height * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
  }
  syncSize();

  const ro = new ResizeObserver(() => syncSize());
  ro.observe(container);

  // ── Render loop ──
  function frame() {
    if (destroyed) return;

    const now = performance.now();
    const t = (now - t0) / 1000;

    // Smooth pointer follow
    pointerX += (targetPX - pointerX) * 0.06;
    pointerY += (targetPY - pointerY) * 0.06;

    const w = canvas.width;
    const h = canvas.height;
    const cx = w / 2;
    const cy = h / 2;
    const minDim = Math.min(w, h);

    // ── Accent colour (single, consistent) ──
    const [acR, acG, acB] = ACCENT;

    // Breathing pulse (0.7 → 1.0)
    const breath = 0.7 + 0.3 * (0.5 + 0.5 * Math.sin(t * 1.8));

    ctx.clearRect(0, 0, w, h);

    // ── LAYER 1: Outer orbital ring (faint, rotating) ──
    const outerR = minDim * 0.42;
    ctx.save();
    ctx.strokeStyle = `rgba(${acR},${acG},${acB},${0.08 * breath})`;
    ctx.lineWidth = 1 * dpr;
    ctx.beginPath();
    ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
    ctx.stroke();
    ctx.restore();

    // ── LAYER 2: Status ring segments (single accent colour, reactive) ──
    const ringR = minDim * 0.36;
    const ringWidth = 3 * dpr;
    const ringRotation = t * 0.15; // slow rotation
    let angleOffset = ringRotation;

    for (const seg of segments) {
      const arcRad = seg.arc * Math.PI * 2;
      const gapRad = GAP * Math.PI * 2;
      const startAngle = angleOffset;
      const endAngle = angleOffset + arcRad;

      // Reactive: pick colour and intensity based on segment state
      const segColor: [number, number, number] = seg.state === 'error' ? ERROR_COLOR : ACCENT;
      const stateAlpha = seg.state === 'inactive' ? 0.2 : 1.0;
      const pulseMult = seg.state === 'error' ? 1.5 : 1.0; // errors pulse stronger

      // Segment pulse — each slightly offset for organic feel
      const segPulse =
        (0.6 + 0.4 * (0.5 + 0.5 * Math.sin(t * 2.0 * pulseMult + angleOffset * 3))) * stateAlpha;

      // Glow pass
      ctx.save();
      ctx.strokeStyle = `rgba(${segColor[0]},${segColor[1]},${segColor[2]},${0.3 * segPulse})`;
      ctx.lineWidth = (ringWidth + 4 * dpr) * segPulse;
      ctx.shadowColor = `rgb(${segColor[0]},${segColor[1]},${segColor[2]})`;
      ctx.shadowBlur = 12 * dpr * segPulse;
      ctx.lineCap = 'round';
      ctx.beginPath();
      ctx.arc(cx, cy, ringR, startAngle, endAngle);
      ctx.stroke();
      ctx.restore();

      // Core pass (bright, thin)
      ctx.save();
      const br = Math.min(255, segColor[0] + 60);
      const bg = Math.min(255, segColor[1] + 60);
      const bb = Math.min(255, segColor[2] + 60);
      ctx.strokeStyle = `rgba(${br},${bg},${bb},${0.7 * segPulse})`;
      ctx.lineWidth = ringWidth;
      ctx.lineCap = 'round';
      ctx.beginPath();
      ctx.arc(cx, cy, ringR, startAngle, endAngle);
      ctx.stroke();
      ctx.restore();

      // Endpoint dots
      for (const angle of [startAngle, endAngle]) {
        const dx = cx + Math.cos(angle) * ringR;
        const dy = cy + Math.sin(angle) * ringR;
        ctx.save();
        ctx.fillStyle = `rgba(${br},${bg},${bb},${0.9 * segPulse})`;
        ctx.shadowColor = `rgb(${segColor[0]},${segColor[1]},${segColor[2]})`;
        ctx.shadowBlur = 6 * dpr * segPulse;
        ctx.beginPath();
        ctx.arc(dx, dy, 2 * dpr, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
      }

      angleOffset = endAngle + gapRad;
    }

    // ── LAYER 3: Inner ring (thin, accent-coloured) ──
    const innerR = minDim * 0.26;
    ctx.save();
    ctx.strokeStyle = `rgba(${acR},${acG},${acB},${0.12 * breath})`;
    ctx.lineWidth = 1 * dpr;
    ctx.setLineDash([4 * dpr, 6 * dpr]);
    ctx.beginPath();
    ctx.arc(cx, cy, innerR, 0, Math.PI * 2);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.restore();

    // ── LAYER 4: Crosshair tick marks (cardinal directions) ──
    const tickLen = 6 * dpr;
    const tickR = minDim * 0.44;
    ctx.save();
    ctx.strokeStyle = `rgba(${acR},${acG},${acB},${0.15 * breath})`;
    ctx.lineWidth = 1 * dpr;
    for (let i = 0; i < 12; i++) {
      const a = (i / 12) * Math.PI * 2 + ringRotation * 0.5;
      const isMajor = i % 3 === 0;
      const len = isMajor ? tickLen * 1.5 : tickLen;
      const x1 = cx + Math.cos(a) * (tickR - len);
      const y1 = cy + Math.sin(a) * (tickR - len);
      const x2 = cx + Math.cos(a) * tickR;
      const y2 = cy + Math.sin(a) * tickR;
      ctx.globalAlpha = isMajor ? 0.25 * breath : 0.12 * breath;
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.restore();

    // ── LAYER 5: Scanning sweep line ──
    const sweepAngle = t * 0.6;
    const sweepLen = minDim * 0.4;
    ctx.save();
    const sweepGrad = ctx.createLinearGradient(
      cx,
      cy,
      cx + Math.cos(sweepAngle) * sweepLen,
      cy + Math.sin(sweepAngle) * sweepLen,
    );
    sweepGrad.addColorStop(0, `rgba(${acR},${acG},${acB},0.15)`);
    sweepGrad.addColorStop(1, `rgba(${acR},${acG},${acB},0)`);
    ctx.strokeStyle = sweepGrad;
    ctx.lineWidth = 1 * dpr;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(cx + Math.cos(sweepAngle) * sweepLen, cy + Math.sin(sweepAngle) * sweepLen);
    ctx.stroke();

    // Sweep trail (fading arc)
    const trailArc = 0.3;
    const trailGrad = ctx.createConicGradient(sweepAngle - trailArc, cx, cy);
    trailGrad.addColorStop(0, `rgba(${acR},${acG},${acB},0)`);
    trailGrad.addColorStop(0.8, `rgba(${acR},${acG},${acB},0.04)`);
    trailGrad.addColorStop(1, `rgba(${acR},${acG},${acB},0.08)`);
    ctx.fillStyle = trailGrad;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, sweepLen, sweepAngle - trailArc, sweepAngle);
    ctx.closePath();
    ctx.fill();
    ctx.restore();

    // ── LAYER 6: Orbiting dots (small satellites on outer ring) ──
    for (let i = 0; i < 3; i++) {
      const orbAngle = t * (0.3 + i * 0.15) + (i * Math.PI * 2) / 3;
      const orbR = outerR + (i % 2 === 0 ? 2 : -2) * dpr;
      const ox = cx + Math.cos(orbAngle) * orbR;
      const oy = cy + Math.sin(orbAngle) * orbR;
      const oPulse = 0.5 + 0.5 * Math.sin(t * 3 + i * 2);
      ctx.save();
      ctx.fillStyle = `rgba(${acR},${acG},${acB},${0.6 * oPulse})`;
      ctx.shadowColor = `rgb(${acR},${acG},${acB})`;
      ctx.shadowBlur = 8 * dpr * oPulse;
      ctx.beginPath();
      ctx.arc(ox, oy, 1.5 * dpr, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }

    // ── LAYER 7: Logo (centred, with pointer parallax + breathing glow) ──
    if (logoReady) {
      const logoSize = minDim * 0.32;
      // Pointer-driven parallax offset
      const logoOffX = pointerX * minDim * 0.02;
      const logoOffY = pointerY * minDim * 0.02;
      const logoX = cx - logoSize / 2 + logoOffX;
      const logoY = cy - logoSize / 2 + logoOffY;

      // Glow behind logo
      ctx.save();
      ctx.shadowColor = `rgb(${acR},${acG},${acB})`;
      ctx.shadowBlur = 25 * dpr * breath;
      ctx.globalAlpha = 0.15 * breath;
      ctx.drawImage(logo, logoX - 2 * dpr, logoY - 2 * dpr, logoSize + 4 * dpr, logoSize + 4 * dpr);
      ctx.restore();

      // Logo proper
      ctx.save();
      ctx.globalAlpha = 0.9;
      ctx.drawImage(logo, logoX, logoY, logoSize, logoSize);
      ctx.restore();
    }

    frameId = requestAnimationFrame(frame);
  }

  frameId = requestAnimationFrame(frame);

  return {
    canvas,
    resize() {
      syncSize();
    },
    setSegmentState(label: string, state: SegmentState) {
      const seg = segments.find((s) => s.label === label);
      if (seg) seg.state = state;
    },
    destroy() {
      destroyed = true;
      cancelAnimationFrame(frameId);
      frameId = 0;
      ro.disconnect();
      canvas.removeEventListener('pointermove', onPointerMove);
      canvas.removeEventListener('pointerleave', onPointerLeave);
      canvas.removeEventListener('touchmove', onTouchMove);
      canvas.remove();
    },
  };
}
