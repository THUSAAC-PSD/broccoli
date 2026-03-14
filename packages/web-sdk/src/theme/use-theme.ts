import { use } from 'react';

import { ThemeContext } from '@/theme/theme-context';

export function useTheme() {
  const context = use(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}
