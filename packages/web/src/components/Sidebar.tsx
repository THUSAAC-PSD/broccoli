import type { ContestProblemResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { useQuery } from '@tanstack/react-query';
import {
  ChevronUp,
  Code2,
  FileText,
  Home,
  LogOut,
  MessageCircle,
  Puzzle,
  Trophy,
  User,
} from 'lucide-react';
import { Link, useLocation } from 'react-router';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
import { useAuth } from '@/contexts/auth-context';
import { useContest } from '@/contexts/contest-context';

const defaultMenuItems = [
  { titleKey: 'sidebar.homepage', icon: Home, url: '/' },
];

const adminMenuItems = [
  { titleKey: 'sidebar.problems', icon: Code2, url: '/problems' },
  { titleKey: 'sidebar.contests', icon: Trophy, url: '/contests' },
  { titleKey: 'sidebar.plugins', icon: Puzzle, url: '/plugins' },
];

function ContestProblemsGroup() {
  const { t } = useTranslation();
  const { contestId: ctxContestId, contestTitle } = useContest();
  const { pathname } = useLocation();
  const apiClient = useApiClient();

  const urlContestId = (() => {
    const m = pathname.match(/^\/contests\/(\d+)/);
    return m ? Number(m[1]) : null;
  })();

  const contestId = ctxContestId ?? urlContestId;

  const { data: contestData } = useQuery({
    queryKey: ['contest', contestId],
    enabled: !!contestId && !contestTitle,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}', {
        params: { path: { id: contestId! } },
      });
      if (error) throw error;
      return data;
    },
  });

  const resolvedTitle = contestTitle ?? contestData?.title ?? null;

  const { data: problems = [] } = useQuery({
    queryKey: ['contest-problems', contestId],
    enabled: !!contestId,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: contestId! } },
      });
      if (error) throw error;
      return data as ContestProblemResponse[];
    },
  });

  if (!contestId || problems.length === 0) return <></>;

  return (
    <SidebarGroup>
      <SidebarGroupLabel>
        {resolvedTitle ?? t('contests.problems')}
      </SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton asChild tooltip={t('sidebar.qa')}>
              <Link to={`/contests/${contestId}/qa`}>
                <MessageCircle />
                <span>{t('sidebar.qa')}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
          {problems.map((p) => (
            <SidebarMenuItem key={p.problem_id}>
              <SidebarMenuButton
                asChild
                tooltip={`${p.label}. ${p.problem_title}`}
              >
                <Link to={`/contests/${contestId}/problems/${p.problem_id}`}>
                  <FileText />
                  <span>
                    {p.label}. {p.problem_title}
                  </span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

export function Sidebar() {
  const { t } = useTranslation();
  const { user, logout } = useAuth();

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
              {user?.role === 'admin' &&
                adminMenuItems.map((item) => {
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

        <ContestProblemsGroup />

        <Slot name="sidebar.groups" as="div" />

        <Slot name="sidebar.content.after" as="div" />
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <Slot name="sidebar.footer" as="div" />
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton>
                  <User className="h-4 w-4" />
                  <span className="flex-1">
                    {user ? user.username : t('sidebar.guest')}
                  </span>
                  <ChevronUp className="ml-auto h-4 w-4" />
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                side="top"
                className="w-[--radix-dropdown-menu-trigger-width]"
              >
                {user ? (
                  <Link to="/">
                    <DropdownMenuItem onClick={logout}>
                      <LogOut className="mr-2 h-4 w-4" />
                      {t('auth.logout')}
                    </DropdownMenuItem>
                  </Link>
                ) : (
                  <DropdownMenuItem asChild>
                    <a href="/login">{t('nav.signIn')}</a>
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
      <SidebarRail />
    </SidebarUI>
  );
}
