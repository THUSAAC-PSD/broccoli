import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'intro',
    {
      type: 'category',
      label: 'Plugins',
      collapsed: false,
      items: ['plugins/printing'],
    },
  ],
};

export default sidebars;
