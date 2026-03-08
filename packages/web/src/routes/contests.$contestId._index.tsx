import type { ContestProblemResponse } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { ChevronRight, Trophy } from 'lucide-react';
import { Link, useParams } from 'react-router';

import { PageLayout } from '@/components/PageLayout';
import { Skeleton } from '@/components/ui/skeleton';
import { useContestInfo } from '@/features/contest/hooks/use-contest-info';

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
    <div>
      <h3 className="text-lg font-semibold mb-3">{t('contests.problems')}</h3>
      {isLoading ? (
        <div className="space-y-2">
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
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
        <div className="rounded-lg border overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="px-4 py-2.5 text-left font-medium text-muted-foreground w-16">
                  {t('problems.label')}
                </th>
                <th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
                  {t('problems.titleColumn')}
                </th>
                <th className="w-10" />
              </tr>
            </thead>
            <tbody className="divide-y">
              {problems.map((p) => (
                <tr
                  key={p.problem_id}
                  className="group transition-colors hover:bg-muted/50"
                >
                  <td className="px-4 py-3">
                    <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary/10 text-xs font-bold text-primary">
                      {p.label}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <Link
                      to={`/contests/${contestId}/problems/${p.problem_id}`}
                      className="font-medium group-hover:text-primary transition-colors"
                    >
                      {p.problem_title}
                    </Link>
                  </td>
                  <td className="px-2 py-3">
                    <ChevronRight className="h-4 w-4 text-muted-foreground/30 group-hover:text-primary transition-colors" />
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

export default function ContestOverviewPage() {
  const { t } = useTranslation();
  const { contestId } = useParams();
  const id = Number(contestId);
  const { contest } = useContestInfo(id);

  if (!contestId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('contests.notFound')}</h1>
      </div>
    );
  }

  return (
    <PageLayout
      pageId="contest-overview"
      title={contest?.title ?? t('contests.title')}
      subtitle={contest?.description}
      icon={<Trophy className="h-6 w-6 text-primary" />}
    >
      <ContestProblemsCard contestId={id} />
    </PageLayout>
  );
}
