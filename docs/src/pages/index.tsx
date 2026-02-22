import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

/* ── Data ──────────────────────────────────────────── */

const avatarSprites = [1, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50];

const pillars = [
  {
    icon: 'lock',
    title: 'Private by default',
    desc: 'Runs fully offline with Ollama. No cloud required, no telemetry, no open ports. Credentials encrypted with AES-256-GCM in your OS keychain. Your data never leaves your machine.',
  },
  {
    icon: 'bolt',
    title: 'Powerful by design',
    desc: 'Multi-agent orchestration, 11 channel bridges, research workflows, DeFi trading, browser automation, 6-stage memory — all powered by an async Rust engine. Not a chat wrapper.',
  },
  {
    icon: 'extension',
    title: 'Extensible by nature',
    desc: 'Connect any OpenAI-compatible provider. Install community skills from the PawzHub. Every feature is modular — use what you need, skip what you don\'t.',
  },
];

const features = [
  { icon: 'smart_toy', title: 'Unlimited Agents', desc: 'Custom personalities, models, and tool policies per agent. Boss/worker orchestration with live delegation.', link: '/docs/guides/agents' },
  { icon: 'forum', title: '11 Chat Bridges', desc: 'Telegram, Discord, Slack, Matrix, IRC, Mattermost, Nostr, Twitch, and more — same brain everywhere.', link: '/docs/channels/overview' },
  { icon: 'psychology', title: 'Hybrid Memory', desc: 'BM25 + vector search, temporal decay, MMR re-ranking, auto-recall. Memory Palace visualization.', link: '/docs/guides/memory' },
  { icon: 'trending_up', title: 'DeFi Trading', desc: 'Uniswap on 7 EVM chains, Jupiter on Solana. Honeypot detection, whale tracking, trading policies.', link: '/docs/guides/trading' },
  { icon: 'web', title: 'Browser Engine', desc: 'Headless Chrome with persistent profiles, screenshot gallery, domain policies, and network filtering.', link: '/docs/guides/browser' },
  { icon: 'science', title: 'Deep Research', desc: 'Quick and deep modes with live source feed, credibility ratings, and auto-synthesized reports.', link: '/docs/guides/research' },
  { icon: 'record_voice_over', title: '35+ TTS Voices', desc: 'Google, OpenAI, ElevenLabs. Talk Mode for continuous voice conversations with your agents.', link: '/docs/guides/voice' },
  { icon: 'task_alt', title: 'Kanban Tasks', desc: 'Drag-and-drop board with agent assignment, cron scheduling, and multi-agent collaboration.', link: '/docs/guides/tasks' },
  { icon: 'mail', title: 'Email Client', desc: 'Full IMAP/SMTP via Himalaya. Per-account permissions with credential audit trail.', link: '/docs/guides/email' },
  { icon: 'shield', title: '7 Security Layers', desc: 'Injection scanning, tool policies, human-in-the-loop, container sandboxing, browser policies.', link: '/docs/reference/security' },
  { icon: 'edit_document', title: 'Content Studio', desc: 'Document editor with markdown/HTML support, word count, and one-click AI improvement.', link: '/docs/guides/content-studio' },
  { icon: 'terminal', title: 'Slash Commands', desc: '/model, /think, /agent, /remember, /recall, /web, /img, /exec, /compact, and more.', link: '/docs/guides/slash-commands' },
];

const providers = [
  'Ollama', 'OpenAI', 'Anthropic', 'Google Gemini',
  'OpenRouter', 'DeepSeek', 'Grok', 'Mistral',
  'Moonshot', 'Any OpenAI-compatible',
];

/* ── Components ────────────────────────────────────── */

function HeroSection() {
  return (
    <header className="hero-section">
      <div className="hero-glow" />
      <div className="hero-glow-2" />
      <div className="container">
        <div className="hero-content">
          <div className="hero-logo-row">
            <img src="/paw/img/pawz-logo.png" alt="OpenPawz" className="hero-logo" />
            <div className="hero-wordmark">
              <span className="hero-wordmark-open">Open</span>
              <span className="hero-wordmark-pawz">Pawz</span>
            </div>
          </div>
          <div className="hero-badge">Open Source &middot; MIT License &middot; Free Forever</div>
          <h1 className="hero-title">
            Your AI, your rules.
          </h1>
          <p className="hero-tagline">
            OpenPawz is a native desktop AI platform that runs fully offline, connects to any provider,
            and puts you in control of every tool call, every message, and every byte of data.
          </p>
          <div className="hero-buttons">
            <Link className="hero-btn hero-btn-primary" to="/docs/start/installation">
              Get Started
            </Link>
            <Link className="hero-btn hero-btn-secondary" to="https://github.com/elisplash/paw">
              View on GitHub
            </Link>
          </div>
          <div className="hero-sub">
            macOS &middot; Linux &middot; Windows &middot; ~5 MB native binary
          </div>
          <div className="hero-avatars">
            {avatarSprites.map((id) => (
              <img key={id} src={`/paw/img/avatars/${id}.png`} alt={`Agent ${id}`} className="hero-avatar" />
            ))}
          </div>
        </div>
      </div>
    </header>
  );
}

function PillarsSection() {
  return (
    <section className="pillars-section">
      <div className="container">
        <div className="pillars-grid">
          {pillars.map((p, i) => (
            <div key={i} className="pillar-card">
              <span className="ms pillar-icon">{p.icon}</span>
              <h3>{p.title}</h3>
              <p>{p.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function FeaturesSection() {
  return (
    <section className="features-section">
      <div className="container">
        <div className="section-header">
          <h2>Everything you need. Nothing you don't.</h2>
          <p>A complete AI workspace — agents, channels, memory, research, trading, browser, and more.</p>
        </div>
        <div className="features-grid">
          {features.map((f, i) => (
            <Link key={i} to={f.link} className="feature-card">
              <span className="ms feature-icon">{f.icon}</span>
              <div>
                <h4>{f.title}</h4>
                <p>{f.desc}</p>
              </div>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}

function ProvidersSection() {
  return (
    <section className="providers-section">
      <div className="container">
        <div className="section-header">
          <h2>Works with every provider.</h2>
          <p>Use one, use all, or bring your own. Switch anytime.</p>
        </div>
        <div className="providers-row">
          {providers.map((p, i) => (
            <span key={i} className="provider-tag">{p}</span>
          ))}
        </div>
      </div>
    </section>
  );
}

function ArchSection() {
  return (
    <section className="arch-section">
      <div className="container">
        <div className="section-header">
          <h2>Native. Not Electron.</h2>
          <p>A Tauri v2 app with a pure async Rust backend. No Node.js server, no open ports, no gateway process.</p>
        </div>
        <div className="arch-flow">
          <div className="arch-node arch-node-input">
            <span className="ms">desktop_windows</span>
            <strong>Desktop App</strong>
            <span className="arch-sub">Tauri v2 &middot; TypeScript UI</span>
          </div>
          <span className="arch-arrow ms">arrow_forward</span>
          <div className="arch-node arch-node-engine">
            <span className="ms">memory</span>
            <strong>OpenPawz Engine</strong>
            <span className="arch-sub">Async Rust + Tokio</span>
            <div className="arch-modules">
              <span>Agents</span><span>Memory</span><span>Security</span>
              <span>Channels</span><span>Trading</span><span>Skills</span>
            </div>
          </div>
          <span className="arch-arrow ms">arrow_forward</span>
          <div className="arch-node arch-node-output">
            <span className="ms">cloud</span>
            <strong>AI Providers</strong>
            <span className="arch-sub">Any OpenAI-compatible API</span>
          </div>
        </div>
        <div className="arch-storage-row">
          <span className="arch-arrow-down ms">arrow_downward</span>
          <div className="arch-node arch-node-store">
            <span className="ms">storage</span>
            <strong>SQLite + OS Keychain</strong>
            <span className="arch-sub">AES-256-GCM encrypted credentials &middot; BM25 + vector memory</span>
          </div>
        </div>
      </div>
    </section>
  );
}

function CTASection() {
  return (
    <section className="cta-section">
      <div className="container">
        <div className="cta-content">
          <img src="/paw/img/pawz-logo.png" alt="OpenPawz" className="cta-logo" />
          <h2>Ready to try it?</h2>
          <p>
            Install OpenPawz, add a provider or start Ollama, and create your first agent.
            Under 5 minutes. Free forever.
          </p>
          <div className="hero-buttons">
            <Link className="hero-btn hero-btn-primary" to="/docs/start/installation">
              Get Started
            </Link>
            <Link className="hero-btn hero-btn-secondary" to="/docs/start/first-agent">
              Create Your First Agent
            </Link>
          </div>
          <div className="cta-trust">
            MIT Licensed &middot; macOS, Linux, Windows
          </div>
        </div>
      </div>
    </section>
  );
}

/* ── Page ──────────────────────────────────────────── */

export default function Home() {
  return (
    <Layout description="OpenPawz — a native desktop AI platform. Private, powerful, extensible. Unlimited providers, 11 channel bridges, multi-agent orchestration. Free and open source.">
      <HeroSection />
      <PillarsSection />
      <FeaturesSection />
      <ProvidersSection />
      <ArchSection />
      <CTASection />
    </Layout>
  );
}
