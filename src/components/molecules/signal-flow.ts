// Signal Flow canvas — live directed-graph visualising Pawz's runtime architecture
// Input nodes (left) → PAWZ ENGINE (centre) → Output nodes (right)
// Animated pulse dots travel along edges to show live data flow direction.

export interface SignalFlowData {
  llmProvider: string;
  llmModel: string;
  embedConnected: boolean;
  embedModel: string;
  mcpServers: { id: string; name: string; connected: boolean; toolCount: number }[];
  n8nRunning: boolean;
  n8nMode: string;
  tailscaleRunning: boolean;
  tailscaleFunnel: boolean;
}

export interface SignalFlowInstance {
  update(data: SignalFlowData): void;
  destroy(): void;
}

// ── internal graph types ─────────────────────────────────────────────
interface SFNode {
  id: string;
  label: string;
  sub: string;
  rx: number; // 0–1 relative x
  ry: number; // 0–1 relative y
  kind: 'input' | 'engine' | 'output';
  active: boolean;
}
interface SFPulse {
  t: number; // 0–1 along edge
  speed: number; // t-units per ms
}
interface SFEdge {
  from: string;
  to: string;
  pulses: SFPulse[];
  active: boolean;
}

// ── colour constants (matches CSS design tokens) ─────────────────────
const C_ACCENT = '#D4654A';
const C_TEXT = 'rgba(255,255,255,0.88)';
const C_SUB = 'rgba(255,255,255,0.38)';
const C_DEAD = 'rgba(255,255,255,0.08)';

// ── bezier point at parameter t ──────────────────────────────────────
function bzPt(
  t: number,
  x0: number,
  y0: number,
  cx: number,
  cy: number,
  x1: number,
  y1: number,
): [number, number] {
  const u = 1 - t;
  return [u * u * x0 + 2 * u * t * cx + t * t * x1, u * u * y0 + 2 * u * t * cy + t * t * y1];
}

// ── cross-browser rounded rect ────────────────────────────────────────
function rr(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

// ── provider display name ─────────────────────────────────────────────
function providerLabel(p: string): string {
  const map: Record<string, string> = {
    anthropic: 'Anthropic',
    openai: 'OpenAI',
    google: 'Google',
    ollama: 'Ollama',
    mistral: 'Mistral',
    groq: 'Groq',
    deepseek: 'DeepSeek',
    grok: 'Grok',
    openrouter: 'OpenRouter',
    moonshot: 'Moonshot',
  };
  return map[p.toLowerCase()] ?? p.slice(0, 12);
}

// ── main factory ─────────────────────────────────────────────────────
export function createSignalFlow(container: HTMLElement): SignalFlowInstance {
  const canvas = document.createElement('canvas');
  canvas.className = 'signal-flow-canvas';
  container.appendChild(canvas);

  let nodes: SFNode[] = [];
  let edges: SFEdge[] = [];
  let rafId = 0;
  let lastT = 0;

  // Per-node persistent pulse phases (so each node breathes independently)
  const _phases = new Map<string, number>();
  const _phase = (id: string) => {
    if (!_phases.has(id)) _phases.set(id, Math.random() * Math.PI * 2);
    return _phases.get(id)!;
  };

  // ── build graph layout ──────────────────────────────────────────
  function buildGraph(data: SignalFlowData | null) {
    nodes = [];
    edges = [];

    if (!data) {
      // skeleton state: show engine + ghost slots
      nodes.push({
        id: 'engine',
        label: 'PAWZ',
        sub: 'ENGINE',
        rx: 0.5,
        ry: 0.5,
        kind: 'engine',
        active: true,
      });
      for (let i = 0; i < 2; i++) {
        nodes.push({
          id: `gl${i}`,
          label: '···',
          sub: '',
          rx: 0.1,
          ry: 0.32 + i * 0.36,
          kind: 'input',
          active: false,
        });
        nodes.push({
          id: `gr${i}`,
          label: '···',
          sub: '',
          rx: 0.9,
          ry: 0.28 + i * 0.44,
          kind: 'output',
          active: false,
        });
        edges.push({ from: `gl${i}`, to: 'engine', pulses: [], active: false });
        edges.push({ from: 'engine', to: `gr${i}`, pulses: [], active: false });
      }
      return;
    }

    // ── input nodes (left) ────────────────────────────────────────
    const inputs: Omit<SFNode, 'rx' | 'ry'>[] = [];
    inputs.push({
      id: 'llm',
      label: data.llmProvider ? providerLabel(data.llmProvider) : 'LLM',
      sub: (data.llmModel || 'no model').slice(0, 18),
      kind: 'input',
      active: !!data.llmProvider,
    });
    if (data.embedConnected) {
      inputs.push({
        id: 'embed',
        label: 'EMBED',
        sub: (data.embedModel || 'vector').slice(0, 18),
        kind: 'input',
        active: true,
      });
    }
    inputs.forEach((n, i) => {
      const c = inputs.length;
      nodes.push({ ...n, rx: 0.11, ry: c === 1 ? 0.5 : 0.3 + (i / (c - 1)) * 0.4 });
    });

    // ── engine node (centre) ──────────────────────────────────────
    nodes.push({
      id: 'engine',
      label: 'PAWZ',
      sub: 'ENGINE',
      rx: 0.5,
      ry: 0.5,
      kind: 'engine',
      active: true,
    });

    // ── output nodes (right) ──────────────────────────────────────
    const outputs: Omit<SFNode, 'rx' | 'ry'>[] = [];
    for (const m of data.mcpServers.slice(0, 5)) {
      outputs.push({
        id: `mcp-${m.id}`,
        label: 'MCP',
        sub: `${m.name.slice(0, 12)}${m.toolCount > 0 ? ` ·${m.toolCount}t` : ''}`,
        kind: 'output',
        active: m.connected,
      });
    }
    if (data.n8nRunning) {
      outputs.push({
        id: 'n8n',
        label: 'n8n',
        sub: data.n8nMode || 'automation',
        kind: 'output',
        active: true,
      });
    }
    if (data.tailscaleRunning) {
      outputs.push({
        id: 'tail',
        label: 'Tailscale',
        sub: data.tailscaleFunnel ? 'funnel' : 'mesh',
        kind: 'output',
        active: true,
      });
    }
    if (outputs.length === 0) {
      outputs.push({
        id: 'empty',
        label: '+ Tools',
        sub: 'connect integrations',
        kind: 'output',
        active: false,
      });
    }
    outputs.forEach((n, i) => {
      const c = outputs.length;
      nodes.push({ ...n, rx: 0.89, ry: c === 1 ? 0.5 : 0.15 + (i / (c - 1)) * 0.7 });
    });

    // ── edges ─────────────────────────────────────────────────────
    for (const n of nodes.filter((x) => x.kind === 'input')) {
      edges.push({
        from: n.id,
        to: 'engine',
        active: n.active,
        pulses: n.active
          ? [
              { t: Math.random(), speed: 0.0016 + Math.random() * 0.0008 },
              { t: (Math.random() + 0.5) % 1, speed: 0.0013 + Math.random() * 0.0008 },
            ]
          : [],
      });
    }
    for (const n of nodes.filter((x) => x.kind === 'output')) {
      edges.push({
        from: 'engine',
        to: n.id,
        active: n.active,
        pulses: n.active
          ? [
              { t: Math.random(), speed: 0.0015 + Math.random() * 0.001 },
              { t: (Math.random() + 0.55) % 1, speed: 0.0012 + Math.random() * 0.001 },
            ]
          : [],
      });
    }
  }

  // ── canvas resize (DPR-aware) ─────────────────────────────────────
  function resize() {
    const dpr = window.devicePixelRatio || 1;
    const cw = container.clientWidth || 420;
    const ch = container.clientHeight || 190;
    canvas.width = Math.round(cw * dpr);
    canvas.height = Math.round(ch * dpr);
    canvas.style.width = `${cw}px`;
    canvas.style.height = `${ch}px`;
    canvas.getContext('2d')!.scale(dpr, dpr);
  }

  // ── animation frame ───────────────────────────────────────────────
  function frame(time: number) {
    const dt = lastT === 0 ? 16 : Math.min(time - lastT, 50);
    lastT = time;

    const cw = container.clientWidth || 420;
    const ch = container.clientHeight || 190;
    const ctx = canvas.getContext('2d')!;
    ctx.clearRect(0, 0, cw, ch);

    // advance pulses
    for (const e of edges) {
      for (const p of e.pulses) p.t = (p.t + p.speed * dt) % 1;
    }

    // ── 1. Ambient nebulae (behind everything) ──────────────────
    const engNode = nodes.find((n) => n.kind === 'engine');
    if (engNode) {
      const ex = engNode.rx * cw;
      const ey = engNode.ry * ch;
      const eb = 0.5 + 0.5 * Math.sin(time * 0.0017 + _phase(engNode.id));
      const engNebula = ctx.createRadialGradient(ex, ey, 0, ex, ey, ch * 0.55);
      engNebula.addColorStop(0, `rgba(212,101,74,${0.1 + eb * 0.06})`);
      engNebula.addColorStop(0.45, 'rgba(212,101,74,0.03)');
      engNebula.addColorStop(1, 'transparent');
      ctx.fillStyle = engNebula;
      ctx.beginPath();
      ctx.arc(ex, ey, ch * 0.55, 0, Math.PI * 2);
      ctx.fill();
    }

    const activeInputs = nodes.filter((n) => n.kind === 'input' && n.active);
    if (activeInputs.length) {
      const ix = (activeInputs.reduce((s, n) => s + n.rx, 0) / activeInputs.length) * cw;
      const iy = (activeInputs.reduce((s, n) => s + n.ry, 0) / activeInputs.length) * ch;
      const inNebula = ctx.createRadialGradient(ix, iy, 0, ix, iy, ch * 0.38);
      inNebula.addColorStop(0, 'rgba(143,176,160,0.07)');
      inNebula.addColorStop(0.65, 'rgba(143,176,160,0.02)');
      inNebula.addColorStop(1, 'transparent');
      ctx.fillStyle = inNebula;
      ctx.beginPath();
      ctx.arc(ix, iy, ch * 0.38, 0, Math.PI * 2);
      ctx.fill();
    }

    const activeOutputs = nodes.filter((n) => n.kind === 'output' && n.active);
    if (activeOutputs.length) {
      const ox = (activeOutputs.reduce((s, n) => s + n.rx, 0) / activeOutputs.length) * cw;
      const oy = (activeOutputs.reduce((s, n) => s + n.ry, 0) / activeOutputs.length) * ch;
      const outNebula = ctx.createRadialGradient(ox, oy, 0, ox, oy, ch * 0.38);
      outNebula.addColorStop(0, 'rgba(143,176,160,0.05)');
      outNebula.addColorStop(0.65, 'rgba(143,176,160,0.015)');
      outNebula.addColorStop(1, 'transparent');
      ctx.fillStyle = outNebula;
      ctx.beginPath();
      ctx.arc(ox, oy, ch * 0.38, 0, Math.PI * 2);
      ctx.fill();
    }

    // ── 2. Edges (behind nodes) ──────────────────────────────────
    for (const edge of edges) {
      const fn = nodes.find((n) => n.id === edge.from);
      const tn = nodes.find((n) => n.id === edge.to);
      if (!fn || !tn) continue;

      const fx = fn.rx * cw;
      const fy = fn.ry * ch;
      const tx = tn.rx * cw;
      const ty = tn.ry * ch;
      // Perpendicular bow — same elegant curve as Memory Palace edges
      const mx = (fx + tx) / 2;
      const my = (fy + ty) / 2;
      const ddx = tx - fx;
      const ddy = ty - fy;
      const elen = Math.sqrt(ddx * ddx + ddy * ddy) || 1;
      const cpx = mx - (ddy / elen) * elen * 0.12;
      const cpy = my + (ddx / elen) * elen * 0.12;

      if (edge.active) {
        // Soft glow underlay on active edges
        ctx.beginPath();
        ctx.moveTo(fx, fy);
        ctx.quadraticCurveTo(cpx, cpy, tx, ty);
        ctx.strokeStyle = 'rgba(143,176,160,0.10)';
        ctx.lineWidth = 6;
        ctx.stroke();
      }

      // Edge line
      ctx.beginPath();
      ctx.moveTo(fx, fy);
      ctx.quadraticCurveTo(cpx, cpy, tx, ty);
      ctx.strokeStyle = edge.active ? 'rgba(143,176,160,0.30)' : C_DEAD;
      ctx.lineWidth = edge.active ? 1 : 0.75;
      if (!edge.active) ctx.setLineDash([3, 5]);
      ctx.stroke();
      ctx.setLineDash([]);

      // Pulse dots — two-layer glow + white-hot core (Memory Palace style)
      for (const p of edge.pulses) {
        const [bx, by] = bzPt(p.t, fx, fy, cpx, cpy, tx, ty);

        // Outer diffuse halo
        const g2 = ctx.createRadialGradient(bx, by, 0, bx, by, 16);
        g2.addColorStop(0, 'rgba(143,176,160,0.25)');
        g2.addColorStop(0.4, 'rgba(143,176,160,0.08)');
        g2.addColorStop(1, 'transparent');
        ctx.beginPath();
        ctx.arc(bx, by, 16, 0, Math.PI * 2);
        ctx.fillStyle = g2;
        ctx.fill();

        // Inner glow
        const g = ctx.createRadialGradient(bx, by, 0, bx, by, 7);
        g.addColorStop(0, 'rgba(143,176,160,0.95)');
        g.addColorStop(0.45, 'rgba(143,176,160,0.30)');
        g.addColorStop(1, 'transparent');
        ctx.beginPath();
        ctx.arc(bx, by, 7, 0, Math.PI * 2);
        ctx.fillStyle = g;
        ctx.fill();

        // White-hot core
        ctx.beginPath();
        ctx.arc(bx, by, 1.8, 0, Math.PI * 2);
        ctx.fillStyle = '#fff';
        ctx.globalAlpha = 0.88;
        ctx.fill();
        ctx.globalAlpha = 1;
      }
    }

    // ── 3. Nodes (on top of edges) ───────────────────────────────
    for (const node of nodes) {
      const nx = node.rx * cw;
      const ny = node.ry * ch;
      const isEng = node.kind === 'engine';
      const nw = isEng ? 68 : 60;
      const nh = isEng ? 38 : 28;
      const pulse = 0.5 + 0.5 * Math.sin(time * 0.0017 + _phase(node.id));

      if (isEng) {
        // Engine: three concentric breathing rings
        ctx.beginPath();
        ctx.arc(nx, ny, 46 + pulse * 8, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(212,101,74,${0.05 + pulse * 0.07})`;
        ctx.lineWidth = 2;
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(nx, ny, 36 + pulse * 5, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(212,101,74,${0.12 + pulse * 0.14})`;
        ctx.lineWidth = 1.5;
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(nx, ny, 28, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(212,101,74,${0.07 + pulse * 0.05})`;
        ctx.lineWidth = 1;
        ctx.stroke();
      } else {
        // Non-engine: outer radial glow halo (Memory Palace node style)
        const glowR = Math.max(nw, nh) * 0.72 + 6 + pulse * 5;
        const haloA = node.active ? 0.16 + pulse * 0.07 : 0.04;
        const halo = ctx.createRadialGradient(nx, ny, 4, nx, ny, glowR);
        halo.addColorStop(
          0,
          node.active ? `rgba(143,176,160,${haloA})` : `rgba(255,255,255,${haloA})`,
        );
        halo.addColorStop(1, 'transparent');
        ctx.fillStyle = halo;
        ctx.beginPath();
        ctx.arc(nx, ny, glowR, 0, Math.PI * 2);
        ctx.fill();
      }

      // Node rect fill
      rr(ctx, nx - nw / 2, ny - nh / 2, nw, nh, 4);
      ctx.fillStyle = isEng
        ? `rgba(212,101,74,${0.09 + pulse * 0.07})`
        : node.active
          ? `rgba(143,176,160,${0.06 + pulse * 0.03})`
          : 'rgba(255,255,255,0.03)';
      ctx.fill();

      // Node border — pulses with the node
      ctx.strokeStyle = isEng
        ? `rgba(212,101,74,${0.65 + pulse * 0.35})`
        : node.active
          ? `rgba(143,176,160,${0.45 + pulse * 0.3})`
          : 'rgba(255,255,255,0.15)';
      ctx.lineWidth = isEng ? 1.5 : 0.8;
      ctx.stroke();

      // Inner gleam (top-left highlight like Memory Palace)
      if (node.active || isEng) {
        rr(ctx, nx - nw / 2 + 2, ny - nh / 2 + 2, nw * 0.55, nh * 0.38, 3);
        ctx.fillStyle = isEng ? 'rgba(255,255,255,0.05)' : 'rgba(255,255,255,0.04)';
        ctx.fill();
      }

      // Label text
      const hasSub = !!node.sub;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillStyle = isEng ? C_ACCENT : node.active ? C_TEXT : C_SUB;
      ctx.font = `${isEng ? 700 : 600} ${isEng ? 10 : 9}px 'JetBrains Mono','Fira Code',ui-monospace,monospace`;
      ctx.fillText(node.label, nx, ny + (hasSub ? -5 : 0));

      if (hasSub) {
        ctx.fillStyle = isEng ? 'rgba(212,101,74,0.62)' : C_SUB;
        ctx.font = `400 7.5px 'JetBrains Mono','Fira Code',ui-monospace,monospace`;
        ctx.fillText(node.sub.length > 18 ? `${node.sub.slice(0, 18)}…` : node.sub, nx, ny + 6);
      }
    }

    rafId = requestAnimationFrame(frame);
  }

  buildGraph(null);
  const ro = new ResizeObserver(resize);
  ro.observe(container);
  resize();
  rafId = requestAnimationFrame(frame);

  return {
    update(data: SignalFlowData) {
      buildGraph(data);
    },
    destroy() {
      cancelAnimationFrame(rafId);
      ro.disconnect();
      canvas.remove();
    },
  };
}
