import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'Pawz',
  tagline: 'The most secure, capable, and extensible AI agent platform for the desktop.',
  favicon: 'img/pawz-favicon.png',

  url: 'https://elisplash.github.io',
  baseUrl: '/paw/',

  organizationName: 'elisplash',
  projectName: 'paw',

  onBrokenLinks: 'throw',

  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/elisplash/paw/tree/main/docs/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    navbar: {
      title: 'Pawz',
      logo: {
        alt: 'Pawz Logo',
        src: 'img/pawz-logo.png',
        width: 32,
        height: 32,
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://github.com/elisplash/paw',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            { label: 'Getting Started', to: '/docs/start/getting-started' },
            { label: 'Architecture', to: '/docs/reference/architecture' },
            { label: 'Security', to: '/docs/reference/security' },
          ],
        },
        {
          title: 'Community',
          items: [
            { label: 'GitHub', href: 'https://github.com/elisplash/paw' },
            { label: 'Issues', href: 'https://github.com/elisplash/paw/issues' },
            { label: 'Discussions', href: 'https://github.com/elisplash/paw/discussions' },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Eli Bury. MIT License.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'rust', 'toml', 'json'],
    },
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
