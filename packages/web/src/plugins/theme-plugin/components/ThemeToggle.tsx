import { useTranslation } from '@broccoli/sdk/i18n';
import { Moon, Sun } from 'lucide-react';

import { SidebarMenuButton, SidebarMenuItem } from '@/components/ui/sidebar';
import { useTheme } from '@/hooks/use-theme';

export function ThemeToggle() {
  const { theme, toggleTheme } = useTheme();
  const { t } = useTranslation();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={toggleTheme}
        tooltip={t('theme.switch')}
        className="bg-sidebar-accent/50 hover:bg-sidebar-accent"
      >
        {theme === 'light' ? <Moon /> : <Sun />}
        <span>{theme === 'light' ? t('theme.dark') : t('theme.light')}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
