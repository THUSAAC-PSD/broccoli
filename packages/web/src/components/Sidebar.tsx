import { Slot } from '@broccoli/sdk/react';
import { BookOpen, Code2, Home, Settings,Trophy, User } from 'lucide-react';

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
  {
    title: 'Dashboard',
    icon: Home,
    url: '#',
  },
  {
    title: 'Problems',
    icon: Code2,
    url: '#',
  },
  {
    title: 'Contests',
    icon: Trophy,
    url: '#',
  },
  {
    title: 'Tutorials',
    icon: BookOpen,
    url: '#',
  },
];

const defaultUserItems = [
  {
    title: 'Profile',
    icon: User,
    url: '#',
  },
  {
    title: 'Settings',
    icon: Settings,
    url: '#',
  },
];

export function Sidebar() {
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
                  <span className="font-semibold">Broccoli OJ</span>
                  <span className="text-xs">Online Judge</span>
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
          <SidebarGroupLabel>Platform</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {defaultMenuItems.map((item) => (
                <SidebarMenuItem key={item.title}>
                  <SidebarMenuButton asChild tooltip={item.title}>
                    <a href={item.url}>
                      <item.icon />
                      <span>{item.title}</span>
                    </a>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
              <Slot name="sidebar.platform.menu" as="div" />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <Slot name="sidebar.groups" as="div" />

        <SidebarGroup>
          <SidebarGroupLabel>Account</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {defaultUserItems.map((item) => (
                <SidebarMenuItem key={item.title}>
                  <SidebarMenuButton asChild tooltip={item.title}>
                    <a href={item.url}>
                      <item.icon />
                      <span>{item.title}</span>
                    </a>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
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
