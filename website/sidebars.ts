import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'intro',
    {
      type: 'category',
      label: 'Using Broccoli',
      collapsed: false,
      items: ['plugins/printing'],
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
