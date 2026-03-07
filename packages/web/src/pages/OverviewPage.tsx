import type {
  ContestListItem,
  ContestProblemResponse,
  SubmissionListItem,
  SubmissionStatus,
  Verdict,
} from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import {
  Activity,
  ArrowRight,
  Clock,
  Code2,
  FileText,
  Home,
  Trophy,
} from 'lucide-react';
import { useEffect } from 'react';
import { Link } from 'react-router';

import { PageLayout } from '@/components/PageLayout';
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
import { useContest } from '@/contexts/contest-context';
import { cn } from '@/lib/utils';

import { ContestInfoCard } from './ContestPage';
import { RankingPage } from './RankingPage';

function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): {
  label: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
} {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now >= start && now <= end)
    return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

function getVerdictBadge(
  verdict: Verdict | null | undefined,
  status: SubmissionStatus,
): {
  label: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
} {
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

function formatRelativeTime(
  dateStr: string,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const mins = Math.floor(diffMs / 60000);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) return t('contests.daysAgo', { count: String(days) });
  if (hours > 0) return t('contests.hoursAgo', { count: String(hours) });
  return t('contests.minutesAgo', { count: String(mins) });
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

function ContestSelector({
  contests,
  onSelect,
}: {
  contests: ContestListItem[];
  onSelect: (contest: ContestListItem) => void;
}) {
  const { t } = useTranslation();

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('dashboard.selectContest')}</CardTitle>
        <CardDescription>{t('dashboard.selectContestDesc')}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {contests.map((contest) => (
            <button
              key={contest.id}
              onClick={() => onSelect(contest)}
              className="flex w-full items-center justify-between rounded-lg border p-4 text-left transition-colors hover:bg-accent"
            >
              <div>
                <div className="font-medium">{contest.title}</div>
                <div className="text-sm text-muted-foreground">
                  {new Date(contest.start_time).toLocaleDateString()} -{' '}
                  {new Date(contest.end_time).toLocaleDateString()}
                </div>
              </div>
            </button>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

function ProblemsTab({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { viewSubmissions } = useContest();

  const {
    data: problems = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data as ContestProblemResponse[];
    },
  });

  if (isLoading) return <ListSkeleton />;
  if (error)
    return (
      <div className="text-sm text-destructive">
        {t('contests.loadProblemsError')}
      </div>
    );
  if (problems.length === 0)
    return (
      <div className="text-sm text-muted-foreground">{t('problems.empty')}</div>
    );

  return (
    <div className="rounded-md border">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b bg-muted/50">
            <th className="px-4 py-3 text-left font-medium w-20">
              {t('problems.label')}
            </th>
            <th className="px-4 py-3 text-left font-medium">
              {t('problems.titleColumn')}
            </th>
            <th className="px-4 py-3 text-right font-medium w-20" />
          </tr>
        </thead>
        <tbody>
          {problems.map((p) => (
            <tr key={p.problem_id} className="border-b last:border-b-0">
              <td className="px-4 py-3 font-semibold">{p.label}</td>
              <td className="px-4 py-3">
                <Link
                  to={`/contests/${contestId}/problems/${p.problem_id}`}
                  className="font-medium hover:text-primary hover:underline"
                >
                  {p.problem_title}
                </Link>
              </td>
              <td className="px-4 py-3 text-right">
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-muted-foreground"
                  onClick={() => viewSubmissions(p.problem_id)}
                  title={t('nav.submissions')}
                >
                  <FileText className="h-3.5 w-3.5" />
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SubmissionsTab({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { filterProblemId, setFilterProblemId } = useContest();

  const { data: problems = [] } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data as ContestProblemResponse[];
    },
  });

  const { data: submissions, isLoading } = useQuery({
    queryKey: ['dashboard-submissions', filterProblemId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions', {
        params: {
          query: {
            page: 1,
            per_page: 20,
            sort_by: 'created_at',
            sort_order: 'desc',
            ...(filterProblemId ? { problem_id: filterProblemId } : {}),
          },
        },
      });
      if (error) throw error;
      return data.data as SubmissionListItem[];
    },
  });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-1.5 flex-wrap">
        <button
          onClick={() => setFilterProblemId(null)}
          className={cn(
            'rounded-full px-3 py-1 text-xs font-medium transition-colors',
            !filterProblemId
              ? 'bg-primary text-primary-foreground'
              : 'bg-muted text-muted-foreground hover:text-foreground',
          )}
        >
          {t('dashboard.all')}
        </button>
        {problems.map((p) => (
          <button
            key={p.problem_id}
            onClick={() => setFilterProblemId(p.problem_id)}
            className={cn(
              'rounded-full px-3 py-1 text-xs font-medium transition-colors',
              filterProblemId === p.problem_id
                ? 'bg-primary text-primary-foreground'
                : 'bg-muted text-muted-foreground hover:text-foreground',
            )}
          >
            {p.label}
          </button>
        ))}
      </div>

      {isLoading ? (
        <ListSkeleton />
      ) : !submissions?.length ? (
        <div className="text-sm text-muted-foreground">
          {t('dashboard.noSubmissions')}
        </div>
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
    </div>
  );
}

export function OverviewPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const { contestId, activeTab, setContest } = useContest();

  const { data: contests, isLoading: isContestsLoading } = useQuery({
    queryKey: ['overview-contests'],
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
      return data.data as ContestListItem[];
    },
  });

  const { data: problems, isLoading: isProblemsLoading } = useQuery({
    queryKey: ['overview-problems'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems', {
        params: {
          query: {
            page: 1,
            per_page: 5,
            sort_by: 'created_at',
            sort_order: 'desc',
          },
        },
      });
      if (error) throw error;
      return data.data;
    },
  });

  const { data: submissions, isLoading: isSubmissionsLoading } = useQuery({
    queryKey: ['overview-submissions', user?.id],
    enabled: !!user,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions', {
        params: {
          query: {
            page: 1,
            per_page: 5,
            sort_by: 'created_at',
            sort_order: 'desc',
          },
        },
      });
      if (error) throw error;
      return data.data as SubmissionListItem[];
    },
  });

  // Auto-select contest if there's exactly one
  useEffect(() => {
    if (contests && contests.length === 1 && !contestId) {
      setContest(contests[0].id, contests[0].title);
    }
  }, [contests, contestId, setContest]);

  // Not logged in
  if (!user) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('dashboard.title')}</h1>
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
      </div>
    );
  }

  // Loading contests
  if (isContestsLoading) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('dashboard.title')}</h1>
        <ListSkeleton />
      </div>
    );
  }

  // No contests
  if (!contests?.length) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('dashboard.title')}</h1>
        <Card>
          <CardContent className="pt-6">
            <p className="text-sm text-muted-foreground">
              {t('dashboard.noActiveContest')}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Multiple contests, none selected yet
  if (contests.length > 1 && !contestId) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('dashboard.title')}</h1>
        <ContestSelector
          contests={contests}
          onSelect={(c) => setContest(c.id, c.title)}
        />
      </div>
    );
  }

  // Contest selected — show tab content if active
  if (contestId && activeTab) {
    if (activeTab === 'problems') {
      return (
        <div className="flex flex-col gap-6 p-6">
          <ContestInfoCard contestId={contestId} />
          <ProblemsTab contestId={contestId} />
        </div>
      );
    }
    if (activeTab === 'submissions') {
      return (
        <div className="flex flex-col gap-6 p-6">
          <SubmissionsTab contestId={contestId} />
        </div>
      );
    }
    if (activeTab === 'ranking') {
      return (
        <div className="flex flex-col gap-6 p-6">
          <RankingPage />
        </div>
      );
    }
  }

  // Default: Overview page with cards
  return (
    <PageLayout
      pageId="overview"
      title={t('overview.title')}
      icon={<Home className="h-6 w-6 text-primary" />}
    >
      <div className="grid gap-6 md:grid-cols-2">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Trophy className="h-4 w-4" />
                {t('overview.contests')}
              </CardTitle>
              <Button variant="ghost" size="sm" asChild>
                <Link to="/contests">
                  {t('overview.viewAll')}
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
                {t('overview.noContests')}
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

        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Code2 className="h-4 w-4" />
                {t('overview.problems')}
              </CardTitle>
              <Button variant="ghost" size="sm" asChild>
                <Link to="/problems">
                  {t('overview.viewAll')}
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
                {t('overview.noProblems')}
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
              {t('overview.recentSubmissions')}
            </CardTitle>
            <CardDescription>
              {t('overview.recentSubmissionsDescription')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isSubmissionsLoading ? (
              <ListSkeleton />
            ) : !submissions?.length ? (
              <p className="text-sm text-muted-foreground">
                {t('overview.noSubmissions')}
              </p>
            ) : (
              <div className="rounded-md border">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="px-4 py-2 text-left font-medium">
                        {t('overview.problem')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('overview.language')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('overview.verdict')}
                      </th>
                      <th className="px-4 py-2 text-left font-medium">
                        {t('overview.submitted')}
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
    </PageLayout>
  );
}
