import type { ContestListItem, SubmissionListItem } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Activity, ArrowRight, Clock, Code2, Home, Trophy } from 'lucide-react';
import { Link } from 'react-router';

import { ListSkeleton } from '@/components/ListSkeleton';
import { PageLayout } from '@/components/PageLayout';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { getContestStatus } from '@/features/contest/utils/status';
import { getVerdictBadge } from '@/features/submission/utils/verdict';
import { formatRelativeDatetime } from '@/lib/utils';

export default function OverviewPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();

  const { data: contests, isLoading: isContestsLoading } = useQuery({
    queryKey: ['overview-contests'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests', {
        params: {
          query: {
            page: 1,
            per_page: 5,
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

        {/* Problems */}
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
                            {formatRelativeDatetime(s.created_at, t)}
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
