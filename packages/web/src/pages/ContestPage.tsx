import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Trophy } from 'lucide-react';
import { Link, useParams } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Markdown } from '@/components/Markdown';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';
import { api } from '@/lib/api/client';
import type { components } from '@/lib/api/schema';

type ContestResponse = components['schemas']['ContestResponse'];
type ContestProblemResponse = components['schemas']['ContestProblemResponse'];

function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' } {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now >= start && now <= end) return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString();
}

export function ContestPage() {
  const { t } = useTranslation();
  const { contestId } = useParams();
  const id = Number(contestId);

  const {
    data: contest,
    isLoading: isContestLoading,
    error: contestError,
  } = useQuery({
    queryKey: ['contest', id],
    enabled: Number.isFinite(id),
    queryFn: async () => {
      const { data, error } = await api.GET('/contests/{id}', {
        params: { path: { id } },
      });
      if (error) throw error;
      return data as ContestResponse;
    },
  });

  const {
    data: problems = [],
    isLoading: isProblemsLoading,
    error: problemsError,
  } = useQuery({
    queryKey: ['contest-problems', id],
    enabled: Number.isFinite(id),
    queryFn: async () => {
      const { data, error } = await api.GET('/contests/{id}/problems', {
        params: { path: { id } },
      });
      if (error) throw error;
      return data as ContestProblemResponse[];
    },
  });

  if (!contestId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('contests.notFound')}</h1>
      </div>
    );
  }

  const status = contest
    ? getContestStatus(contest.start_time, contest.end_time, t)
    : null;

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Trophy className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">
          {contest?.title ?? t('contests.title')}
        </h1>
      </div>

      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-4">
            <CardTitle className="text-xl">
              {contest?.title ?? t('contests.title')}
            </CardTitle>
            {status && <Badge variant={status.variant}>{status.label}</Badge>}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {isContestLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-5 w-64" />
              <Skeleton className="h-5 w-48" />
              <Skeleton className="h-24 w-full" />
            </div>
          ) : contestError ? (
            <div className="text-sm text-destructive">
              {t('contests.loadError')}
            </div>
          ) : contest ? (
            <>
              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <div className="text-sm text-muted-foreground">
                    {t('contests.startTime')}
                  </div>
                  <div className="font-medium">
                    {formatDateTime(contest.start_time)}
                  </div>
                </div>
                <div>
                  <div className="text-sm text-muted-foreground">
                    {t('contests.endTime')}
                  </div>
                  <div className="font-medium">
                    {formatDateTime(contest.end_time)}
                  </div>
                </div>
              </div>
              <Separator />
              <div>
                <div className="text-sm text-muted-foreground">
                  {t('contests.description')}
                </div>
                <div className="prose prose-sm dark:prose-invert max-w-none">
                  <Markdown>{contest.description || t('contests.noDescription')}</Markdown>
                </div>
              </div>
            </>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('contests.problems')}</CardTitle>
        </CardHeader>
        <CardContent>
          {isProblemsLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-6 w-full" />
              <Skeleton className="h-6 w-full" />
              <Skeleton className="h-6 w-full" />
            </div>
          ) : problemsError ? (
            <div className="text-sm text-destructive">
              {t('contests.loadProblemsError')}
            </div>
          ) : problems.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              {t('problems.empty')}
            </div>
          ) : (
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
                  </tr>
                </thead>
                <tbody>
                  {problems.map((p) => (
                    <tr key={p.problem_id} className="border-b">
                      <td className="px-4 py-3 font-semibold">{p.label}</td>
                      <td className="px-4 py-3">
                        <Link
                          to={`/contests/${id}/problems/${p.problem_id}`}
                          className="font-medium hover:text-primary hover:underline"
                        >
                          {p.problem_title}
                        </Link>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
