import type * as Preset from '@docusaurus/preset-classic';
import type { Config } from '@docusaurus/types';
import { themes as prismThemes } from 'prism-react-renderer';

const config: Config = {
  title: 'Broccoli',
  tagline: 'A contest judge platform',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://docs.broccoli.run',
  baseUrl: '/',

  organizationName: 'THUSAAC-PSD',
  projectName: 'broccoli',

  onBrokenLinks: 'throw',

  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'zh-CN'],
    localeConfigs: {
      en: { label: 'English' },
      'zh-CN': { label: '简体中文' },
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl:
            'https://github.com/THUSAAC-PSD/broccoli/tree/master/website/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        language: ['en', 'zh'],
        indexDocs: true,
        indexBlog: false,
        docsRouteBasePath: '/',
      },
    ],
  ],

  plugins: [
    'docusaurus-plugin-image-zoom',
    'docusaurus-plugin-copy-page-button',
    [
      '@aeorank/docusaurus',
      {
        siteName: 'Broccoli',
        siteUrl: 'https://docs.broccoli.run/',
        description: 'Documentation for the Broccoli contest judge platform.',
        organization: {
          name: 'THUSAAC-PSD',
          url: 'https://github.com/THUSAAC-PSD/broccoli',
        },
      },
    ],
  ],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    zoom: {
      selector: '.markdown img',
      background: {
        light: 'rgb(255, 255, 255)',
        dark: 'rgb(24, 24, 27)',
      },
    },
    navbar: {
      title: 'Broccoli',
      logo: {
        alt: 'Broccoli',
        src: 'img/logo.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docs',
          position: 'left',
          label: 'Docs',
        },
        { type: 'localeDropdown', position: 'right' },
        {
          href: 'https://github.com/THUSAAC-PSD/broccoli',
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
          items: [{ label: 'Printing', to: '/plugins/printing' }],
        },
        {
          title: 'More',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/THUSAAC-PSD/broccoli',
            },
          ],
        },
      ],
      copyright: 'Built with Docusaurus.',
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
