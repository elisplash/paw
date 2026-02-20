import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'category',
      label: 'Getting Started',
      items: [
        'start/getting-started',
        'start/installation',
        'start/first-agent',
        'start/first-provider',
      ],
    },
    {
      type: 'category',
      label: 'Guides',
      items: [
        'guides/agents',
        'guides/memory',
        'guides/tasks',
        'guides/skills',
        'guides/voice',
        'guides/research',
        'guides/orchestrator',
        'guides/email',
        'guides/trading',
        'guides/slash-commands',
        'guides/automations',
        'guides/browser',
        'guides/tailscale',
        'guides/sessions',
        'guides/foundry',
        'guides/dashboard',
        'guides/projects',
        'guides/content-studio',
        'guides/container-sandbox',
        'guides/pricing',
      ],
    },
    {
      type: 'category',
      label: 'Channels',
      items: [
        'channels/overview',
        'channels/telegram',
        'channels/discord',
        'channels/slack',
        'channels/matrix',
        'channels/irc',
        'channels/mattermost',
        'channels/nextcloud',
        'channels/nostr',
        'channels/twitch',
        'channels/webchat',
      ],
    },
    {
      type: 'category',
      label: 'Providers',
      items: [
        'providers/overview',
        'providers/ollama',
        'providers/openai',
        'providers/anthropic',
        'providers/google',
        'providers/openrouter',
        'providers/deepseek',
        'providers/grok',
        'providers/mistral',
        'providers/moonshot',
        'providers/custom',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: [
        'reference/architecture',
        'reference/security',
        'reference/troubleshooting',
      ],
    },
  ],
};

export default sidebars;
