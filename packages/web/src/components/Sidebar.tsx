import { useApiClient } from '@broccoli/web-sdk/api';
import { useAuth, USER_PERMISSIONS } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
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
} from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import {
  Activity,
  BarChart3,
  ChevronUp,
  Code2,
  Home,
  Inbox,
  LayoutGrid,
  LogOut,
  MessageCircle,
  Presentation,
  Puzzle,
  Server,
  Trophy,
  User,
  Users,
} from 'lucide-react';
import { Link, useLocation } from 'react-router';

import { useContest } from '@/features/contest/contexts/contest-context';

import logo from '../../resources/Logo.png';
import { LocaleSelector } from './LocaleSelector';
import { ThemeToggle } from './ThemeSwitcher';

interface MenuItem {
  key: string;
  icon: React.ComponentType<React.SVGProps<SVGSVGElement>>;
  url: string;
  exact: boolean;
  requiredPermissions?: string[];
}

const adminMenuItems: MenuItem[] = [
  {
    key: 'sidebar.dashboard',
    icon: Home,
    url: '/admin',
    exact: true,
    requiredPermissions: [
      'user:manage',
      'problem:create',
      'contest:manage',
      'plugin:manage',
    ],
  },
  {
    key: 'sidebar.users',
    icon: Users,
    url: '/admin/users',
    exact: false,
    requiredPermissions: ['user:manage', 'role:manage'],
  },
  {
    key: 'sidebar.problems',
    icon: Code2,
    url: '/problems',
    exact: false,
    requiredPermissions: ['problem:create', 'problem:edit', 'problem:delete'],
  },
  {
    key: 'sidebar.contests',
    icon: Trophy,
    url: '/contests',
    exact: false,
    requiredPermissions: ['contest:manage'],
  },
  {
    key: 'sidebar.plugins',
    icon: Puzzle,
    url: '/admin/plugins',
    exact: false,
    requiredPermissions: ['plugin:manage'],
  },
  {
    key: 'sidebar.allSubmissions',
    icon: Activity,
    url: '/admin/submissions',
    exact: false,
    requiredPermissions: ['submission:view_all'],
  },
  {
    key: 'sidebar.system',
    icon: Server,
    url: '/admin/system',
    exact: false,
    requiredPermissions: ['system:view'],
  },
  {
    key: 'sidebar.dlq',
    icon: Inbox,
    url: '/admin/dlq',
    exact: false,
    requiredPermissions: ['dlq:manage'],
  },
];

const filterByPermissions = (
  items: MenuItem[],
  permissions: string[],
): MenuItem[] => {
  return items.filter((item) => {
    if (!item.requiredPermissions) return true;
    return item.requiredPermissions.some((perm) => permissions.includes(perm));
  });
};

function MenuGroupItems({
  items,
  pathname,
  t,
}: {
  items: MenuItem[];
  pathname: string;
  t: (k: string) => string;
}) {
  return (
    <>
      {items.map(({ key, icon: Icon, url, exact }) => {
        const title = t(key);
        const active = exact ? pathname === url : pathname.startsWith(url);
        return (
          <SidebarMenuItem key={key}>
            <SidebarMenuButton asChild isActive={active} tooltip={title}>
              <Link to={url}>
                <Icon className={active ? 'text-sidebar-primary' : undefined} />
                <span>{title}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        );
      })}
    </>
  );
}

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
      return data;
    },
  });

  if (!contestId) return <></>;

  return (
    <SidebarGroup>
      <SidebarGroupLabel>
        {resolvedTitle ?? t('contests.problems')}
      </SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          <MenuGroupItems
            items={[
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
            ]}
            pathname={pathname}
            t={t}
          />
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

function PlatformGroup() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const { pathname } = useLocation();
  const apiClient = useApiClient();
  const permissions = user?.permissions || [];
  const adminItems = filterByPermissions(adminMenuItems, permissions);

  const { data: contests } = useQuery({
    queryKey: ['dashboard-contests'],
    enabled: !!user,
    staleTime: 5 * 60 * 1000,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests', {
        params: {
          query: {
            page: 1,
            per_page: 100,
            sort_by: 'start_time',
            sort_order: 'desc',
          },
        },
      });
      if (error) throw error;
      return data.data;
    },
  });

  const multipleContests = contests ? contests.length > 1 : false;
  const havePermissions = user
    ? USER_PERMISSIONS.some((perm) => user.permissions.includes(perm))
    : false;

  return (
    <SidebarGroup>
      {(multipleContests || havePermissions) && (
        <SidebarGroupLabel>
          {havePermissions ? t('sidebar.admin') : t('sidebar.platform')}
        </SidebarGroupLabel>
      )}
      <SidebarGroupContent>
        <SidebarMenu>
          {multipleContests && !havePermissions && (
            <SidebarMenuItem key="sidebar.selector">
              <SidebarMenuButton asChild isActive={pathname === '/'}>
                <Link to="/">
                  <LayoutGrid
                    className={
                      pathname === '/' ? 'text-sidebar-primary' : undefined
                    }
                  />
                  <span>{t('sidebar.selector')}</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          )}
          <MenuGroupItems items={adminItems} pathname={pathname} t={t} />
          <Slot name="sidebar.platform.menu" as="div" />
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
            <SidebarMenuButton size="lg" asChild className="h-auto p-0">
              <Link to="/">
                <img
                  src={logo}
                  className="flex items-center justify-center w-full"
                />
                {/* <div className="flex flex-col gap-0.5 leading-none">
                  <span className="font-semibold">{t('app.name')}</span>
                  <span className="text-xs">{t('app.tagline')}</span>
                </div>
                */}
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <Slot name="sidebar.header" as="div" />
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <Slot name="sidebar.content.before" as="div" />
        <PlatformGroup />

        <ContestProblemsGroup />

        <Slot name="sidebar.groups" as="div" />

        <Slot name="sidebar.content.after" as="div" />
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <Slot name="sidebar.footer" as="div" />
          <LocaleSelector />
          <ThemeToggle />
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
                className="w-(--radix-dropdown-menu-trigger-width)"
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
                    <Link to="/login">{t('nav.signIn')}</Link>
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
