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
const C_SAGE = '#8FB0A0';
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

    // ── draw edges (behind nodes) ───────────────────────────────
    for (const edge of edges) {
      const fn = nodes.find((n) => n.id === edge.from);
      const tn = nodes.find((n) => n.id === edge.to);
      if (!fn || !tn) continue;

      const fx = fn.rx * cw;
      const fy = fn.ry * ch;
      const tx = tn.rx * cw;
      const ty = tn.ry * ch;
      // slight upward bow proportional to horizontal span
      const cpx = (fx + tx) / 2;
      const cpy = (fy + ty) / 2 - (tx - fx) * 0.07;

      // edge line
      ctx.beginPath();
      ctx.moveTo(fx, fy);
      ctx.quadraticCurveTo(cpx, cpy, tx, ty);
      ctx.strokeStyle = edge.active ? 'rgba(143,176,160,0.20)' : C_DEAD;
      ctx.lineWidth = 0.75;
      if (!edge.active) ctx.setLineDash([3, 5]);
      ctx.stroke();
      ctx.setLineDash([]);

      // pulse dots
      for (const p of edge.pulses) {
        const [bx, by] = bzPt(p.t, fx, fy, cpx, cpy, tx, ty);
        // glow halo
        const g = ctx.createRadialGradient(bx, by, 0, bx, by, 7);
        g.addColorStop(0, 'rgba(143,176,160,0.90)');
        g.addColorStop(0.45, 'rgba(143,176,160,0.28)');
        g.addColorStop(1, 'transparent');
        ctx.beginPath();
        ctx.arc(bx, by, 7, 0, Math.PI * 2);
        ctx.fillStyle = g;
        ctx.fill();
        // hard core
        ctx.beginPath();
        ctx.arc(bx, by, 2, 0, Math.PI * 2);
        ctx.fillStyle = C_SAGE;
        ctx.fill();
      }
    }

    // ── draw nodes (on top of edges) ────────────────────────────
    for (const node of nodes) {
      const nx = node.rx * cw;
      const ny = node.ry * ch;
      const isEng = node.kind === 'engine';
      const nw = isEng ? 68 : 60;
      const nh = isEng ? 38 : 28;

      // engine breathing ring
      if (isEng) {
        const b = 0.5 + 0.5 * Math.sin(time * 0.0017);
        ctx.beginPath();
        ctx.arc(nx, ny, 34 + b * 5, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(212,101,74,${0.12 + b * 0.15})`;
        ctx.lineWidth = 1.5;
        ctx.stroke();
        // static inner ring
        ctx.beginPath();
        ctx.arc(nx, ny, 28, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(212,101,74,0.07)';
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      // node fill + border
      rr(ctx, nx - nw / 2, ny - nh / 2, nw, nh, 4);
      ctx.fillStyle = isEng
        ? 'rgba(212,101,74,0.12)'
        : node.active
          ? 'rgba(143,176,160,0.07)'
          : 'rgba(255,255,255,0.03)';
      ctx.fill();
      ctx.strokeStyle = isEng ? C_ACCENT : node.active ? C_SAGE : 'rgba(255,255,255,0.18)';
      ctx.lineWidth = isEng ? 1.5 : 0.8;
      ctx.stroke();

      // label text
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
