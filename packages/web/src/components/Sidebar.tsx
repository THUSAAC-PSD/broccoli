import type { ContestProblemResponse } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/react';
import { useQuery } from '@tanstack/react-query';
import {
  BarChart3,
  ChevronUp,
  Code2,
  Home,
  LogOut,
  MessageCircle,
  Presentation,
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
import { useAuth } from '@/features/auth/hooks/use-auth';
import { useContest } from '@/features/contest/contexts/contest-context';

interface MenuItem {
  titleKey: string;
  icon: React.ComponentType<React.SVGProps<SVGSVGElement>>;
  url: string;
  requiredPermissions?: string[];
}

const allMenuItems: MenuItem[] = [
  {
    titleKey: 'sidebar.admin',
    icon: Home,
    url: '/admin',
    requiredPermissions: [
      'user:manage',
      'problem:create',
      'contest:manage',
      'plugin:manage',
    ],
  },
  {
    titleKey: 'sidebar.problems',
    icon: Code2,
    url: '/problems',
    requiredPermissions: ['problem:create', 'problem:edit', 'problem:delete'],
  },
  {
    titleKey: 'sidebar.contests',
    icon: Trophy,
    url: '/contests',
    requiredPermissions: ['contest:manage'],
  },
  {
    titleKey: 'sidebar.plugins',
    icon: Puzzle,
    url: '/plugins',
    requiredPermissions: ['plugin:manage'],
  },
];

const getMenuItems = (permissions: string[]): MenuItem[] => {
  return allMenuItems.filter((item) => {
    if (!item.requiredPermissions) return true;
    return item.requiredPermissions.some((perm) => permissions.includes(perm));
  });
};

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
          {[
            {
              key: 'sidebar.contestshomepage',
              icon: Presentation,
              url: `/contests/${contestId}`,
              exact: true,
            },
            {
              key: 'sidebar.qa',
              icon: MessageCircle,
              url: `/contests/${contestId}/qa`,
              exact: false,
            },
            {
              key: 'sidebar.submissions',
              icon: Code2,
              url: `/contests/${contestId}/submissions`,
              exact: false,
            },
            {
              key: 'sidebar.rankings',
              icon: BarChart3,
              url: `/contests/${contestId}/rankings`,
              exact: false,
            },
          ].map(({ key, icon: Icon, url, exact }) => {
            const active = exact ? pathname === url : pathname.startsWith(url);
            return (
              <SidebarMenuItem key={key}>
                <SidebarMenuButton asChild isActive={active} tooltip={t(key)}>
                  <Link to={url}>
                    <Icon
                      className={active ? 'text-sidebar-primary' : undefined}
                    />
                    <span>{t(key)}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
          {problems.map((p) => {
            const isActive =
              pathname === `/contests/${contestId}/problems/${p.problem_id}`;
            return (
              <SidebarMenuItem key={p.problem_id}>
                <SidebarMenuButton
                  asChild
                  isActive={isActive}
                  tooltip={`${p.label}. ${p.problem_title}`}
                >
                  <Link to={`/contests/${contestId}/problems/${p.problem_id}`}>
                    <span className="relative flex size-4 shrink-0 items-center justify-center">
                      <span
                        className={`absolute flex size-5 items-center justify-center rounded text-[11px] font-bold leading-none ${
                          isActive
                            ? 'bg-sidebar-primary text-sidebar-primary-foreground'
                            : 'bg-sidebar-foreground/10 text-sidebar-foreground/60'
                        }`}
                      >
                        {p.label}
                      </span>
                    </span>
                    <span>{p.problem_title}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function isActivePath(pathname: string, url: string) {
  if (url === '/') return pathname === '/';
  return pathname.startsWith(url);
}

export function Sidebar() {
  const { t } = useTranslation();
  const { user, logout } = useAuth();
  const { pathname } = useLocation();
  const permissions = user?.permissions || [];
  const menuItems = getMenuItems(permissions);

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
              {menuItems.map((item) => {
                const title = t(item.titleKey);
                const active = isActivePath(pathname, item.url);
                return (
                  <SidebarMenuItem key={item.titleKey}>
                    <SidebarMenuButton
                      asChild
                      isActive={active}
                      tooltip={title}
                    >
                      <Link to={item.url}>
                        <item.icon
                          className={
                            active ? 'text-sidebar-primary' : undefined
                          }
                        />
                        <span>{title}</span>
                      </Link>
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
