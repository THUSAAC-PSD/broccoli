import { useApiClient } from '@broccoli/web-sdk/api';
import type { ContestSummary } from '@broccoli/web-sdk/contest';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Badge, Button } from '@broccoli/web-sdk/ui';
import { formatDateTime } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import { CalendarDays, ChevronRight, Code2, Trophy } from 'lucide-react';
import { useEffect } from 'react';
import { Link, useNavigate } from 'react-router';

import { ListSkeleton } from '@/components/ListSkeleton';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { useContest } from '@/features/contest/contexts/contest-context';
import { getContestStatus } from '@/features/contest/utils/status';

function ContestSelector({ contests }: { contests: ContestSummary[] }) {
  const { t, locale } = useTranslation();
  const navigate = useNavigate();

  return (
    <div>
      <h2 className="text-lg font-semibold mb-3">
        {t('homepage.selectContest')}
      </h2>
      <p className="text-sm text-muted-foreground mb-4">
        {t('homepage.selectContestDesc')}
      </p>
      <div className="space-y-2">
        {contests.map((contest) => {
          const { label, variant } = getContestStatus(
            contest.start_time,
            contest.end_time,
            t,
          );
          return (
            <button
              key={contest.id}
              onClick={() => navigate(`/contests/${contest.id}`)}
              className="group flex w-full items-center gap-4 rounded-lg border p-4 text-left transition-colors hover:bg-muted/50"
            >
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                <Trophy className="h-5 w-5 text-primary" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="font-medium">{contest.title}</div>
                <div className="text-xs text-muted-foreground mt-0.5 flex items-center gap-1.5">
                  <CalendarDays className="h-3 w-3" />
                  {formatDateTime(contest.start_time, locale)} —{' '}
                  {formatDateTime(contest.end_time, locale)}
                </div>
              </div>
              <Badge variant={variant} className="shrink-0">
                {label}
              </Badge>
              <ChevronRight className="h-4 w-4 text-muted-foreground/30 group-hover:text-primary transition-colors shrink-0" />
            </button>
          );
        })}
      </div>
    </div>
  );
}

function GuestWelcome() {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] text-center px-4">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/10 mb-6">
        <Code2 className="h-8 w-8 text-primary" />
      </div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">
        {t('homepage.welcome')}
      </h1>
      <p className="text-muted-foreground max-w-md mb-8">
        {t('homepage.welcomeDesc')}
      </p>
      <div className="flex gap-3">
        <Button size="lg" asChild>
          <Link to="/login">{t('nav.signIn')}</Link>
        </Button>
        <Button size="lg" variant="outline" asChild>
          <Link to="/register">{t('nav.signUp')}</Link>
        </Button>
      </div>
    </div>
  );
}

export default function Index() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const { contestId, setContest } = useContest();
  const navigate = useNavigate();

  const { data: contests, isLoading: isContestsLoading } = useQuery({
    queryKey: ['dashboard-contests'],
    enabled: !!user,
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
      return data.data as ContestSummary[];
    },
  });

  // Auto-select contest if there's exactly one
  useEffect(() => {
    if (!user) {
      return;
    }
    if (user && user.role === 'admin') {
      navigate('/admin');
      return;
    }
    if (contests && contests.length === 1 && !contestId) {
      setContest(contests[0].id, contests[0].title);
      navigate(`/contests/${contests[0].id}`);
    }
  }, [contests, contestId, setContest]);

  // Admin user, redirect to admin dashboard
  if (user && user.role === 'admin') {
    return <></>;
  }

  // Avoid rendering homepage content when we're about to auto-redirect
  if (contests && contests.length === 1 && user && !contestId) {
    return null;
  }

  // Not logged in
  if (!user) {
    return <GuestWelcome />;
  }

  // Loading contests
  if (isContestsLoading) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('homepage.title')}</h1>
        <ListSkeleton />
      </div>
    );
  }

  if (contests && contests.length === 1) {
    return <></>;
  }

  // No contests
  if (!contests?.length) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh] text-center px-4">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted mb-6">
          <Trophy className="h-8 w-8 text-muted-foreground" />
        </div>
        <h1 className="text-2xl font-bold tracking-tight mb-2">
          {t('homepage.title')}
        </h1>
        <p className="text-muted-foreground max-w-md">
          {t('homepage.noContests')}
        </p>
      </div>
    );
  }

  // Multiple contests
  if (contests.length > 1 && !contestId) {
    return (
      <div className="flex flex-col gap-6 p-6 max-w-2xl mx-auto">
        <ContestSelector contests={contests} />
      </div>
    );
  }
}
