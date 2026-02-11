import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { BookOpen, Code2, Home, Settings, Trophy, User } from 'lucide-react';

import {
  Sidebar as SidebarUI,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from '@/components/ui/sidebar';

const defaultMenuItems = [
  { titleKey: 'sidebar.dashboard', icon: Home, url: '#' },
  { titleKey: 'sidebar.problems', icon: Code2, url: '#' },
  { titleKey: 'sidebar.contests', icon: Trophy, url: '#' },
  { titleKey: 'sidebar.tutorials', icon: BookOpen, url: '#' },
];

const defaultUserItems = [
  { titleKey: 'sidebar.profile', icon: User, url: '#' },
  { titleKey: 'sidebar.settings', icon: Settings, url: '#' },
];

export function Sidebar() {
  const { t } = useTranslation();

  return (
    <SidebarUI collapsible="icon">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <a href="#">
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                  <Code2 className="size-4" />
                </div>
                <div className="flex flex-col gap-0.5 leading-none">
                  <span className="font-semibold">{t('app.name')}</span>
                  <span className="text-xs">{t('app.tagline')}</span>
                </div>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <Slot name="sidebar.header" as="div" />
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <Slot name="sidebar.content.before" as="div" />

        <SidebarGroup>
          <SidebarGroupLabel>{t('sidebar.platform')}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {defaultMenuItems.map((item) => {
                const title = t(item.titleKey);
                return (
                  <SidebarMenuItem key={item.titleKey}>
                    <SidebarMenuButton asChild tooltip={title}>
                      <a href={item.url}>
                        <item.icon />
                        <span>{title}</span>
                      </a>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })}
              <Slot name="sidebar.platform.menu" as="div" />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <Slot name="sidebar.groups" as="div" />

        <SidebarGroup>
          <SidebarGroupLabel>{t('sidebar.account')}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {defaultUserItems.map((item) => {
                const title = t(item.titleKey);
                return (
                  <SidebarMenuItem key={item.titleKey}>
                    <SidebarMenuButton asChild tooltip={title}>
                      <a href={item.url}>
                        <item.icon />
                        <span>{title}</span>
                      </a>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })}
              <Slot name="sidebar.account.menu" as="div" />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <Slot name="sidebar.content.after" as="div" />
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <Slot name="sidebar.footer" as="div" />
          <SidebarMenuItem>
            <SidebarMenuButton>
              <User className="mr-2 h-4 w-4" />
              <span className="flex-1">John Doe</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
      <SidebarRail />
    </SidebarUI>
  );
}
