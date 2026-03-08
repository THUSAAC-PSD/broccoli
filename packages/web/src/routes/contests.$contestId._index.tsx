import type { ContestProblemResponse } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import {
  AlertTriangle,
  AlignLeft,
  CalendarClock,
  Check,
  Clock,
  Cpu,
  Trophy,
  X,
} from 'lucide-react';
import { Link, useParams } from 'react-router';

import { Markdown } from '@/components/Markdown';
import { PageLayout } from '@/components/PageLayout';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';
import { useContestInfo } from '@/features/contest/hooks/use-contest-info';
import { getContestStatus } from '@/features/contest/utils/status';
import { formatDateTime } from '@/lib/utils';

type MockVerdict =
  | 'Accept'
  | 'Wrong Answer'
  | 'Not Submitted'
  | 'Time Limit Exceeded'
  | 'Memory Limit Exceeded'
  | 'Runtime Error';

type MockProblemType =
  | 'Traditional'
  | 'Interactive'
  | 'Communication'
  | 'Output Only'
  | 'Submit Answer';

type MockProblemMeta = {
  verdict: MockVerdict;
  type: MockProblemType;
};

const MOCK_PROBLEM_TYPES: MockProblemType[] = [
  'Traditional',
  'Interactive',
  'Communication',
  'Output Only',
  'Submit Answer',
];

function getMockProblemMeta(problemId: number, index: number): MockProblemMeta {
  const seed = Math.abs(problemId * 97 + index * 31);
  const ratio = seed % 100;

  // Weighted distribution close to typical contest overview statistics.
  let verdict: MockVerdict;
  if (ratio < 42) verdict = 'Accept';
  else if (ratio < 77) verdict = 'Wrong Answer';
  else if (ratio < 94) verdict = 'Not Submitted';
  else if (ratio < 97) verdict = 'Time Limit Exceeded';
  else if (ratio < 99) verdict = 'Memory Limit Exceeded';
  else verdict = 'Runtime Error';

  return {
    verdict,
    type: MOCK_PROBLEM_TYPES[(seed + 2) % MOCK_PROBLEM_TYPES.length],
  };
}

function getVerdictTextClassName(verdict: MockVerdict): string {
  if (verdict === 'Accept') {
    return 'text-emerald-600 dark:text-emerald-400';
  }
  if (verdict === 'Wrong Answer') {
    return 'text-rose-600 dark:text-rose-400';
  }
  if (verdict === 'Time Limit Exceeded') {
    return 'text-amber-600 dark:text-amber-400';
  }
  if (verdict === 'Memory Limit Exceeded') {
    return 'text-blue-600 dark:text-blue-400';
  }
  return 'text-slate-600 dark:text-slate-400';
}

function getVerdictIcon(verdict: MockVerdict) {
  if (verdict === 'Accept') return <Check className="size-4" />;
  if (verdict === 'Wrong Answer') return <X className="size-4" />;
  if (verdict === 'Time Limit Exceeded') return <Clock className="size-4" />;
  if (verdict === 'Memory Limit Exceeded') return <Cpu className="size-4" />;
  return <AlertTriangle className="size-4" />;
}

function renderVerdict(verdict: MockVerdict, compact = false) {
  if (verdict === 'Not Submitted') return null;

  return (
    <span
      className={`inline-flex items-center gap-1.5 ${compact ? 'text-base' : 'text-base'} font-medium ${getVerdictTextClassName(verdict)}`}
    >
      {getVerdictIcon(verdict)}
      {verdict}
    </span>
  );
}

export function ContestInfoCard({ contestId }: { contestId: number }) {
  const { locale } = useTranslation();
  const { contest, isLoading, error } = useContestInfo(contestId);

  const toEnglishStatus = (key: string): string => {
    if (key === 'contests.upcoming') return 'Upcoming';
    if (key === 'contests.running') return 'Running';
    if (key === 'contests.ended') return 'Ended';
    return key;
  };

  const status = contest
    ? getContestStatus(contest.start_time, contest.end_time, toEnglishStatus)
    : null;

  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Clock className="h-4 w-4" />
            {'Schedule'}
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
            Failed to load contest.
          </div>
        ) : contest ? (
          <>
            <div className="grid gap-4 sm:grid-cols-2 rounded-lg border bg-muted/30 p-4">
              <div className="flex items-center gap-3">
                <div className="rounded-md bg-background p-2 shadow-sm">
                  <CalendarClock className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <div className="text-sm text-muted-foreground">Start</div>
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
                  <div className="text-sm text-muted-foreground">End</div>
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
                Description
              </div>
              <div className="prose prose-sm dark:prose-invert max-w-none rounded-lg border bg-muted/10 p-4">
                <Markdown>{contest.description || 'No description.'}</Markdown>
              </div>
            </div>
          </>
        ) : null}
      </CardContent>
    </Card>
  );
}

export function ContestProblemsCard({ contestId }: { contestId: number }) {
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

  const problemsWithMeta = problems.map((problem, index) => ({
    problem,
    meta: getMockProblemMeta(problem.problem_id, index),
  }));

  return (
    <div>
      {isLoading ? (
        <div className="space-y-3">
          <Skeleton className="h-6 w-full" />
          <Skeleton className="h-6 w-full" />
          <Skeleton className="h-6 w-full" />
        </div>
      ) : error ? (
        <div className="text-sm text-destructive">Failed to load problems.</div>
      ) : problems.length === 0 ? (
        <div className="text-sm text-muted-foreground">No problems.</div>
      ) : (
        <div className="rounded-md border overflow-x-auto">
          <table className="w-full text-base min-w-[760px]">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="px-4 py-3 text-left font-bold w-20">Label</th>
                <th className="px-4 py-3 text-left font-bold">Title</th>
                <th className="px-4 py-3 text-left font-bold w-48">
                  <span className="inline-flex items-center gap-1.5">Type</span>
                </th>
                <th className="px-4 py-3 text-left font-bold w-64">
                  <span className="inline-flex items-center gap-1.5">
                    Verdict
                  </span>
                </th>
              </tr>
            </thead>
            <tbody>
              {problemsWithMeta.map(({ problem: p, meta }) => (
                <tr key={p.problem_id} className="border-b">
                  <td className="px-4 py-3 text-base font-semibold align-top">
                    {p.label}
                  </td>
                  <td className="px-4 py-3">
                    <Link
                      to={`/contests/${contestId}/problems/${p.problem_id}`}
                      className="text-base font-medium hover:text-primary hover:underline"
                    >
                      {p.problem_title}
                    </Link>
                  </td>
                  <td className="px-4 py-3 align-top">
                    <Badge variant="outline">{meta.type}</Badge>
                  </td>
                  <td className="px-4 py-3 align-top">
                    {renderVerdict(meta.verdict)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export function ContestProblemsCardGrid({ contestId }: { contestId: number }) {
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

  const problemsWithMeta = problems.map((problem, index) => ({
    problem,
    meta: getMockProblemMeta(problem.problem_id, index),
  }));

  return (
    <div>
      {isLoading ? (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          <Skeleton className="h-44 w-full" />
          <Skeleton className="h-44 w-full" />
          <Skeleton className="h-44 w-full" />
        </div>
      ) : error ? (
        <div className="text-sm text-destructive">Failed to load problems.</div>
      ) : problems.length === 0 ? (
        <div className="text-sm text-muted-foreground">No problems.</div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {problemsWithMeta.map(({ problem: p, meta }) => (
            <Card
              key={p.problem_id}
              className="h-full border-muted-foreground/20"
            >
              <CardHeader className="space-y-3 pb-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xl font-extrabold uppercase tracking-wide text-foreground">
                    {p.label}
                  </span>
                  <Badge variant="outline" className="text-sm">
                    {meta.type}
                  </Badge>
                </div>
                <Link
                  to={`/contests/${contestId}/problems/${p.problem_id}`}
                  className="line-clamp-2 min-h-11 text-base font-semibold leading-6 hover:underline"
                >
                  {p.problem_title}
                </Link>
              </CardHeader>
              <CardContent className="pt-0">
                {renderVerdict(meta.verdict, true)}
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

export default function ContestOverviewPage() {
  const { contestId } = useParams();
  const id = Number(contestId);
  const { contest } = useContestInfo(id);

  if (!contestId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">Contest not found</h1>
      </div>
    );
  }

  return (
    <PageLayout
      pageId="contest-overview"
      title={contest?.title ?? 'Contest'}
      icon={<Trophy className="h-6 w-6 text-primary" />}
    >
      <ContestInfoCard contestId={id} />
      <ContestProblemsCard contestId={id} />
      <ContestProblemsCardGrid contestId={id} />
    </PageLayout>
  );
}
