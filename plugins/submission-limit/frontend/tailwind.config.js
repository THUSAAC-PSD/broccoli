import { broccoliPreset } from '@broccoli/web-sdk/tailwind';

export default {
  presets: [broccoliPreset],
  content: ['./src/**/*.{js,ts,jsx,tsx}'],
};
