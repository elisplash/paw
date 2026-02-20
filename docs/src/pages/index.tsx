import clsx from 'clsx';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary')} style={{textAlign: 'center', padding: '4rem 0'}}>
      <div className="container">
        <h1 className="hero__title">{siteConfig.title}</h1>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div style={{display: 'flex', gap: '1rem', justifyContent: 'center', marginTop: '1.5rem'}}>
          <Link className="button button--secondary button--lg" to="/docs/start/getting-started">
            Get Started
          </Link>
          <Link className="button button--outline button--lg" style={{color: 'white', borderColor: 'white'}} to="/docs/guides/agents">
            Explore Guides
          </Link>
        </div>
      </div>
    </header>
  );
}

function Feature({title, description}: {title: string, description: string}) {
  return (
    <div className="col col--4" style={{marginBottom: '2rem'}}>
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

export default function Home() {
  return (
    <Layout description="Documentation for Pawz — a native desktop AI agent platform">
      <HomepageHeader />
      <main style={{padding: '3rem 0'}}>
        <div className="container">
          <div className="row">
            <Feature title="Multi-Agent System" description="Create unlimited agents with custom personalities, models, and tool policies. Boss/worker orchestration with sub-agent spawning." />
            <Feature title="10 AI Providers" description="Ollama, OpenAI, Anthropic, Google, OpenRouter, DeepSeek, xAI, Mistral, Moonshot, and any OpenAI-compatible endpoint." />
            <Feature title="10 Channel Bridges" description="Telegram, Discord, Slack, Matrix, IRC, Mattermost, Nextcloud Talk, Nostr, Twitch, and embeddable WebChat." />
            <Feature title="Defense-in-Depth Security" description="Command risk classifier, HIL approval, prompt injection scanner, container sandboxing, OS keychain, AES-256 encryption." />
            <Feature title="Semantic Memory" description="Long-term memory with Ollama embeddings, hybrid BM25 + vector search, temporal decay, and a Memory Palace visualization." />
            <Feature title="37+ Built-in Skills" description="Email, GitHub, trading, TTS, image generation, smart home, and more — all with encrypted credential injection." />
          </div>
        </div>
      </main>
    </Layout>
  );
}
