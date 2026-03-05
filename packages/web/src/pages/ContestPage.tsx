import type { ContestProblemResponse, ContestResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { useQuery } from '@tanstack/react-query';
import { AlignLeft, CalendarClock, Clock, Trophy } from 'lucide-react';
import { Link, useParams } from 'react-router';

import { Markdown } from '@/components/Markdown';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';

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

function formatDateTime(dateStr: string, locale?: string): string {
  return new Date(dateStr).toLocaleString(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function useContestData(contestId: number) {
  const apiClient = useApiClient();
  const {
    data: contest,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['contest', contestId],
    enabled: Number.isFinite(contestId),
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data as ContestResponse;
    },
  });
  return { contest, isLoading, error };
}

export function ContestInfoCard({ contestId }: { contestId: number }) {
  const { t, locale } = useTranslation();
  const { contest, isLoading, error } = useContestData(contestId);

  const status = contest
    ? getContestStatus(contest.start_time, contest.end_time, t)
    : null;

  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Clock className="h-4 w-4" />
            {'时间'}
          </div>
          {status && <Badge variant={status.variant}>{status.label}</Badge>}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {isLoading ? (
          <div className="space-y-3">
            <Skeleton className="h-5 w-64" />
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : error ? (
          <div className="text-sm text-destructive">
            {t('contests.loadError')}
          </div>
        ) : contest ? (
          <>
            <div className="grid gap-4 sm:grid-cols-2 rounded-lg border bg-muted/30 p-4">
              <div className="flex items-center gap-3">
                <div className="rounded-md bg-background p-2 shadow-sm">
                  <CalendarClock className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <div className="text-sm text-muted-foreground">
                    {t('contests.startTime')}
                  </div>
                  <div className="font-semibold text-base mt-0.5">
                    {formatDateTime(contest.start_time, locale)}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <div className="rounded-md bg-background p-2 shadow-sm">
                  <CalendarClock className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <div className="text-sm text-muted-foreground">
                    {t('contests.endTime')}
                  </div>
                  <div className="font-semibold text-base mt-0.5">
                    {formatDateTime(contest.end_time, locale)}
                  </div>
                </div>
              </div>
            </div>

            <Separator className="my-2" />

            <div className="space-y-3">
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <AlignLeft className="h-4 w-4" />
                {t('contests.description')}
              </div>
              <div className="prose prose-sm dark:prose-invert max-w-none rounded-lg border bg-muted/10 p-4">
                <Markdown>
                  {contest.description || t('contests.noDescription')}
                </Markdown>
              </div>
            </div>
          </>
        ) : null}
      </CardContent>
    </Card>
  );
}

export function ContestProblemsCard({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const apiClient = useApiClient();

  const {
    data: problems = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: ['contest-problems', contestId],
    enabled: Number.isFinite(contestId),
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data as ContestProblemResponse[];
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('contests.problems')}</CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-3">
            <Skeleton className="h-6 w-full" />
            <Skeleton className="h-6 w-full" />
            <Skeleton className="h-6 w-full" />
          </div>
        ) : error ? (
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
                        to={`/contests/${contestId}/problems/${p.problem_id}`}
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
  );
}

export function ContestPage() {
  const { t } = useTranslation();
  const { contestId } = useParams();
  const id = Number(contestId);
  const { contest } = useContestData(id);

  if (!contestId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('contests.notFound')}</h1>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Trophy className="h-6 w-6 text-primary" />
          <h1 className="text-2xl font-bold">
            {contest?.title ?? t('contests.title')}
          </h1>
        </div>
      </div>
      <Slot name="contest-detail.header" as="div" />

      <ContestInfoCard contestId={id} />
      <ContestProblemsCard contestId={id} />

      <Slot name="contest-detail.scoreboard" as="div" />
    </div>
  );
}
