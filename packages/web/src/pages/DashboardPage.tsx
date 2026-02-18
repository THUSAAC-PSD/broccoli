import type {
  ContestListItem,
  SubmissionListItem,
  SubmissionStatus,
  Verdict,
} from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Activity, ArrowRight, Clock, Code2, Home, Trophy } from 'lucide-react';
import { Link } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { useAuth } from '@/contexts/auth-context';

function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): { label: string; variant: 'default' | 'secondary' | 'outline' } {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now <= end) return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

function formatRelativeTime(
  dateStr: string,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = date.getTime() - now.getTime();
  const absDiffMs = Math.abs(diffMs);
  const mins = Math.floor(absDiffMs / 60000);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);

  if (diffMs > 0) {
    if (days > 0) return t('contests.inDays', { count: String(days) });
    if (hours > 0) return t('contests.inHours', { count: String(hours) });
    return t('contests.inMinutes', { count: String(mins) });
  }
  if (days > 0) return t('contests.daysAgo', { count: String(days) });
  if (hours > 0) return t('contests.hoursAgo', { count: String(hours) });
  return t('contests.minutesAgo', { count: String(mins) });
}

function getVerdictBadge(
  verdict: Verdict | null | undefined,
  status: SubmissionStatus,
): { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' } {
  if (status === 'Pending' || status === 'Compiling' || status === 'Running') {
    return { label: status, variant: 'outline' };
  }
  if (status === 'CompilationError') {
    return { label: 'CE', variant: 'secondary' };
  }
  if (status === 'SystemError') {
    return { label: 'SE', variant: 'secondary' };
  }
  if (!verdict) {
    return { label: status, variant: 'outline' };
  }
  if (verdict === 'Accepted') {
    return { label: 'AC', variant: 'default' };
  }
  const shortNames: Record<string, string> = {
    WrongAnswer: 'WA',
    TimeLimitExceeded: 'TLE',
    MemoryLimitExceeded: 'MLE',
    RuntimeError: 'RE',
    SystemError: 'SE',
  };
  return { label: shortNames[verdict] ?? verdict, variant: 'destructive' };
}

function ListSkeleton() {
  return (
    <div className="space-y-3">
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-full" />
    </div>
  );
}

export function DashboardPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();

  const { data: contests, isLoading: isContestsLoading } = useQuery({
    queryKey: ['dashboard-contests'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests', {
        params: {
          query: { page: 1, per_page: 5, sort_by: 'start_time', sort_order: 'desc' },
        },
      });
      if (error) throw error;
      return data.data as ContestListItem[];
    },
  });

  const { data: problems, isLoading: isProblemsLoading } = useQuery({
    queryKey: ['dashboard-problems'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems', {
        params: {
          query: { page: 1, per_page: 5, sort_by: 'created_at', sort_order: 'desc' },
        },
      });
      if (error) throw error;
      return data.data;
    },
  });

  const { data: submissions, isLoading: isSubmissionsLoading } = useQuery({
    queryKey: ['dashboard-submissions', user?.id],
    enabled: !!user,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions', {
        params: {
          query: { page: 1, per_page: 5, sort_by: 'created_at', sort_order: 'desc' },
        },
      });
      if (error) throw error;
      return data.data as SubmissionListItem[];
    },
  });

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Home className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{t('dashboard.title')}</h1>
      </div>

      {!user && (
        <Card>
          <CardHeader>
            <CardTitle>{t('dashboard.welcome')}</CardTitle>
            <CardDescription>
              {t('dashboard.welcomeDescription')}
            </CardDescription>
          </CardHeader>
          <CardFooter className="gap-2">
            <Button asChild>
              <Link to="/login">{t('nav.signIn')}</Link>
            </Button>
            <Button variant="outline" asChild>
              <Link to="/register">{t('nav.signUp')}</Link>
            </Button>
          </CardFooter>
        </Card>
      )}

      <div className="grid gap-6 md:grid-cols-2">
        {/* Contests */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Trophy className="h-4 w-4" />
                {t('dashboard.contests')}
              </CardTitle>
              <Button variant="ghost" size="sm" asChild>
                <Link to="/contests">
                  {t('dashboard.viewAll')}
                  <ArrowRight className="ml-1 h-3 w-3" />
                </Link>
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {isContestsLoading ? (
              <ListSkeleton />
            ) : !contests?.length ? (
              <p className="text-sm text-muted-foreground">
                {t('dashboard.noContests')}
              </p>
            ) : (
              <div className="space-y-3">
                {contests.map((contest) => {
                  const status = getContestStatus(
                    contest.start_time,
                    contest.end_time,
                    t,
                  );
                  return (
                    <div
                      key={contest.id}
                      className="flex items-center justify-between gap-2"
                    >
                      <Link
                        to={`/contests/${contest.id}`}
                        className="text-sm font-medium hover:text-primary hover:underline truncate"
                      >
                        {contest.title}
                      </Link>
                      <Badge variant={status.variant} className="shrink-0">
                        {status.label}
                      </Badge>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Problems */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Code2 className="h-4 w-4" />
                {t('dashboard.problems')}
              </CardTitle>
              <Button variant="ghost" size="sm" asChild>
                <Link to="/problems">
                  {t('dashboard.viewAll')}
                  <ArrowRight className="ml-1 h-3 w-3" />
                </Link>
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {isProblemsLoading ? (
              <ListSkeleton />
            ) : !problems?.length ? (
              <p className="text-sm text-muted-foreground">
                {t('dashboard.noProblems')}
              </p>
            ) : (
              <div className="space-y-3">
                {problems.map((problem) => (
                  <div
                    key={problem.id}
                    className="flex items-center justify-between gap-2"
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-xs text-muted-foreground shrink-0">
                        #{problem.id}
                      </span>
                      <Link
                        to={`/problems/${problem.id}`}
                        className="text-sm font-medium hover:text-primary hover:underline truncate"
                      >
                        {problem.title}
                      </Link>
                    </div>
                    <div className="flex items-center gap-1 text-xs text-muted-foreground shrink-0">
                      <Clock className="h-3 w-3" />
                      {problem.time_limit}ms
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Recent Submissions (logged-in only) */}
      {user && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="h-4 w-4" />
              {t('dashboard.recentSubmissions')}
            </CardTitle>
            <CardDescription>
              {t('dashboard.recentSubmissionsDescription')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isSubmissionsLoading ? (
              <ListSkeleton />
            ) : !submissions?.length ? (
              <p className="text-sm text-muted-foreground">
                {t('dashboard.noSubmissions')}
              </p>
            ) : (
              <div className="rounded-md border">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="px-4 py-2 text-left font-medium">
                        {t('dashboard.problem')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('dashboard.language')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('dashboard.verdict')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('dashboard.submitted')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {submissions.map((s) => {
                      const vb = getVerdictBadge(s.verdict, s.status);
                      return (
                        <tr key={s.id} className="border-b last:border-b-0">
                          <td className="px-4 py-2">
                            <Link
                              to={`/problems/${s.problem_id}`}
                              className="font-medium hover:text-primary hover:underline"
                            >
                              {s.problem_title}
                            </Link>
                          </td>
                          <td className="px-4 py-2">
                            <Badge variant="outline">{s.language}</Badge>
                          </td>
                          <td className="px-4 py-2">
                            <Badge variant={vb.variant}>{vb.label}</Badge>
                          </td>
                          <td className="px-4 py-2 text-muted-foreground">
                            {formatRelativeTime(s.created_at, t)}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
