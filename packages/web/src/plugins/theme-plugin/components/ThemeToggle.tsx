import { Moon, Sun } from 'lucide-react';

import { SidebarMenuButton,SidebarMenuItem } from '@/components/ui/sidebar';
import { useTheme } from '@/hooks/use-theme';

export function ThemeToggle() {
  const { theme, toggleTheme } = useTheme();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={toggleTheme}
        tooltip="Switch theme"
        className="bg-sidebar-accent/50 hover:bg-sidebar-accent"
      >
        {theme === 'light' ? <Moon /> : <Sun />}
        <span>{theme === 'light' ? 'Dark' : 'Light'} Mode</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
