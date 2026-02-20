import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

const features = [
  {
    icon: '🤖',
    title: 'Multi-Agent',
    description: 'Create unlimited agents with custom personalities, models, and tool policies. Boss/worker orchestration for complex projects.',
    link: '/docs/guides/agents',
  },
  {
    icon: '🔌',
    title: '10 Providers',
    description: 'Ollama, OpenAI, Anthropic, Google Gemini, OpenRouter, DeepSeek, Grok, Mistral, Moonshot, or any OpenAI-compatible API.',
    link: '/docs/providers/overview',
  },
  {
    icon: '💬',
    title: '10 Channels',
    description: 'Telegram, Discord, Slack, Matrix, IRC, Mattermost, Nextcloud, Nostr, Twitch, and embeddable WebChat.',
    link: '/docs/channels/overview',
  },
  {
    icon: '🛡️',
    title: 'Security First',
    description: 'HIL approval, prompt injection scanning, container sandboxing, credential vault with OS keychain encryption.',
    link: '/docs/reference/security',
  },
  {
    icon: '🧠',
    title: 'Memory',
    description: 'Long-term memory with hybrid BM25 + vector search, temporal decay, auto-capture, and a Memory Palace UI.',
    link: '/docs/guides/memory',
  },
  {
    icon: '⚡',
    title: '37+ Skills',
    description: 'Email, GitHub, trading, TTS, image gen, smart home, Spotify, and more — with encrypted credential injection.',
    link: '/docs/guides/skills',
  },
];

const quickLinks = [
  { label: 'Agents', to: '/docs/guides/agents', icon: '🤖' },
  { label: 'Channels', to: '/docs/channels/overview', icon: '💬' },
  { label: 'Providers', to: '/docs/providers/overview', icon: '🔌' },
  { label: 'Skills', to: '/docs/guides/skills', icon: '⚡' },
  { label: 'Memory', to: '/docs/guides/memory', icon: '🧠' },
  { label: 'Orchestrator', to: '/docs/guides/orchestrator', icon: '🎯' },
  { label: 'Security', to: '/docs/reference/security', icon: '🛡️' },
  { label: 'Architecture', to: '/docs/reference/architecture', icon: '🏗️' },
];

function HeroSection() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <header className="hero-section">
      <div className="hero-glow" />
      <div className="container">
        <div className="hero-content">
          <div className="hero-badge">Open Source &middot; MIT License</div>
          <h1 className="hero-title">
            <span className="hero-title-accent">Pawz</span>
          </h1>
          <p className="hero-tagline">{siteConfig.tagline}</p>
          <p className="hero-description">
            Run AI agents locally with Ollama or connect to 10 cloud providers.
            Bridge to Telegram, Discord, Slack, and 7 more platforms.
            Built with Tauri, Rust, and TypeScript.
          </p>
          <div className="hero-buttons">
            <Link className="hero-btn hero-btn-primary" to="/docs/start/getting-started">
              Get Started
            </Link>
            <Link className="hero-btn hero-btn-secondary" to="https://github.com/elisplash/paw">
              GitHub
            </Link>
          </div>
        </div>
      </div>
    </header>
  );
}

function FeaturesSection() {
  return (
    <section className="features-section">
      <div className="container">
        <div className="section-header">
          <h2>Everything you need</h2>
          <p>A complete platform for building, running, and managing AI agents on your desktop.</p>
        </div>
        <div className="features-grid">
          {features.map((f, i) => (
            <Link key={i} to={f.link} className="feature-card">
              <div className="feature-icon">{f.icon}</div>
              <h3>{f.title}</h3>
              <p>{f.description}</p>
              <span className="feature-link">Learn more &rarr;</span>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}

function QuickLinksSection() {
  return (
    <section className="quicklinks-section">
      <div className="container">
        <div className="section-header">
          <h2>Quick links</h2>
          <p>Jump straight to what you need.</p>
        </div>
        <div className="quicklinks-grid">
          {quickLinks.map((l, i) => (
            <Link key={i} to={l.to} className="quicklink-card">
              <span className="quicklink-icon">{l.icon}</span>
              <span className="quicklink-label">{l.label}</span>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}

function StatsSection() {
  return (
    <section className="stats-section">
      <div className="container">
        <div className="stats-grid">
          <div className="stat">
            <div className="stat-number">10</div>
            <div className="stat-label">AI Providers</div>
          </div>
          <div className="stat">
            <div className="stat-number">10</div>
            <div className="stat-label">Channel Bridges</div>
          </div>
          <div className="stat">
            <div className="stat-number">37+</div>
            <div className="stat-label">Built-in Skills</div>
          </div>
          <div className="stat">
            <div className="stat-number">30</div>
            <div className="stat-label">Agent Tools</div>
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
          <h2>Ready to get started?</h2>
          <p>Install Pawz and create your first agent in under 5 minutes.</p>
          <div className="hero-buttons">
            <Link className="hero-btn hero-btn-primary" to="/docs/start/installation">
              Installation Guide
            </Link>
            <Link className="hero-btn hero-btn-secondary" to="/docs/start/first-agent">
              Create Your First Agent
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home() {
  return (
    <Layout description="Documentation for Pawz — a native desktop AI agent platform">
      <HeroSection />
      <StatsSection />
      <FeaturesSection />
      <QuickLinksSection />
      <CTASection />
    </Layout>
  );
}
