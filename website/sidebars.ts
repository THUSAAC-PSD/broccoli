import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'intro',
    'downloads',
    {
      type: 'category',
      label: 'Using Broccoli',
      collapsed: false,
      items: ['cli/contestant', 'plugins/printing'],
    },
    {
      type: 'category',
      label: 'Building plugins',
      collapsed: false,
      items: ['building-plugins/getting-started'],
    },
  ],
};

export default sidebars;
