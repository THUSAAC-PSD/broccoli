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
import { FileText } from 'lucide-react';
import { useEffect } from 'react';
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
import { cn } from '@/lib/utils';
import { Skeleton } from '@/components/ui/skeleton';
import { useAuth } from '@/contexts/auth-context';
import { useContest } from '@/contexts/contest-context';

import { RankingPage } from './RankingPage';

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
      <div className="text-sm text-muted-foreground">
        {t('problems.empty')}
      </div>
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

export function DashboardPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const { contestId, activeTab, setContest } = useContest();

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
      return data.data as ContestListItem[];
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

  // Contest selected — show tab content
  return (
    <div className="flex flex-col gap-6 p-6">
      {activeTab === 'problems' && contestId && (
        <ProblemsTab contestId={contestId} />
      )}
      {activeTab === 'submissions' && contestId && (
        <SubmissionsTab contestId={contestId} />
      )}
      {activeTab === 'ranking' && <RankingPage />}
    </div>
  );
}
