import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useTheme } from '@broccoli/web-sdk/theme';
import { SidebarMenuButton, SidebarMenuItem } from '@broccoli/web-sdk/ui';
import { Moon, Sun } from 'lucide-react';

export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  const toggleTheme = () => {
    setTheme(theme === 'light' ? 'dark' : 'light');
  };
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
