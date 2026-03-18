import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  getStatusLabel,
  SUBMISSION_STATUS_FILTER_OPTIONS,
  type SubmissionStatusFilterValue,
  toSubmissionStatus,
} from '@broccoli/web-sdk/submission';
import { Button, FilterDropdown } from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Code2,
  ListFilter,
  Search,
} from 'lucide-react';
import { useMemo, useState } from 'react';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { fetchContestProblemList } from '@/features/contest/api/fetch-contest-problem-list';
import { useContest } from '@/features/contest/contexts/contest-context';
import { fetchSupportedLanguages } from '@/features/problem/api/fetch-supported-languages';
import { fetchContestSubmissions } from '@/features/submission/api/fetch-contest-submissions';

import { SubmissionsTable } from './SubmissionsTable';

const PER_PAGE = 20;

export function ContestSubmissions({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const { filterProblemId, setFilterProblemId } = useContest();

  const [draftProblemId, setDraftProblemId] = useState<string>(
    filterProblemId ? String(filterProblemId) : 'all',
  );
  const [draftLanguage, setDraftLanguage] = useState<string>('all');
  const [draftStatus, setDraftStatus] =
    useState<SubmissionStatusFilterValue>('all');

  const [appliedLanguage, setAppliedLanguage] = useState<string | null>(null);
  const [appliedStatus, setAppliedStatus] =
    useState<SubmissionStatusFilterValue>('all');
  const [page, setPage] = useState(1);

  const { data: problems = [] } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: () => fetchContestProblemList(apiClient, contestId),
  });

  const { data: supportedLanguages = [] } = useQuery({
    queryKey: ['supported-languages'],
    queryFn: () => fetchSupportedLanguages(apiClient),
    staleTime: 5 * 60 * 1000,
  });

  const { data: submissionsResult, isLoading } = useQuery({
    queryKey: [
      'contest-submissions-table',
      String(contestId),
      String(filterProblemId ?? 'all'),
      String(appliedLanguage ?? 'all'),
      appliedStatus,
      page,
    ],
    enabled: !!user,
    queryFn: () =>
      fetchContestSubmissions(apiClient, {
        contestId,
        problemId: filterProblemId,
        language: appliedLanguage,
        status: toSubmissionStatus(appliedStatus),
        userId: user?.id,
        page,
        per_page: PER_PAGE,
        sort_by: 'created_at',
        sort_order: 'desc',
      }),
  });

  const submissions = submissionsResult?.data ?? [];
  const pagination = submissionsResult?.pagination;

  const problemOptions = useMemo(
    () => [
      { value: 'all', label: t('submissions.filters.allProblems') },
      ...problems.map((p) => ({
        value: String(p.problem_id),
        label: `${p.label}. ${p.problem_title}`,
      })),
    ],
    [problems, t],
  );

  const languageOptions = useMemo(
    () => [
      { value: 'all', label: t('submissions.filters.allLanguages') },
      ...supportedLanguages.map((l) => ({
        value: l.id,
        label: l.name,
      })),
    ],
    [supportedLanguages, t],
  );

  const statusOptions = useMemo(
    () =>
      SUBMISSION_STATUS_FILTER_OPTIONS.map((s) => ({
        value: s,
        label: getStatusLabel(s, t),
      })),
    [t],
  );

  const applyFilters = () => {
    const nextProblemId =
      draftProblemId === 'all' ? null : Number(draftProblemId);
    setFilterProblemId(nextProblemId);
    setAppliedLanguage(draftLanguage === 'all' ? null : draftLanguage);
    setAppliedStatus(draftStatus);
    setPage(1);
  };

  return (
    <div className="flex flex-col gap-4">
      {/* Filters */}
      <div className="flex flex-wrap items-center gap-3">
        <FilterDropdown
          icon={<BookOpen className="h-4 w-4" />}
          value={draftProblemId}
          options={problemOptions}
          onChange={setDraftProblemId}
          className="w-[260px]"
        />
        <FilterDropdown
          icon={<Code2 className="h-4 w-4" />}
          value={draftLanguage}
          options={languageOptions}
          onChange={setDraftLanguage}
          className="w-[220px]"
        />
        <FilterDropdown
          icon={<ListFilter className="h-4 w-4" />}
          value={draftStatus}
          options={statusOptions}
          onChange={(next) =>
            setDraftStatus(next as SubmissionStatusFilterValue)
          }
          className="w-[220px]"
        />
        <Button className="h-9 shrink-0" onClick={applyFilters}>
          <Search className="mr-1 h-4 w-4" />
          {t('submissions.filters.search')}
        </Button>
      </div>

      {/* Table */}
      <div className="border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="flex items-center justify-center py-16">
            <span className="h-5 w-5 rounded-full border-2 border-primary border-t-transparent animate-spin" />
          </div>
        ) : (
          <SubmissionsTable
            submissions={submissions}
            columns={SubmissionsTable.fullColumns}
            linkBuilder={(sub) =>
              `/contests/${contestId}/submissions/${sub.id}`
            }
          />
        )}
      </div>

      {/* Pagination */}
      {pagination && pagination.total_pages > 1 && (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span className="tabular-nums">
            {(page - 1) * PER_PAGE + 1}–
            {Math.min(page * PER_PAGE, pagination.total)} of {pagination.total}
          </span>
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              disabled={page <= 1}
              onClick={() => setPage((p) => p - 1)}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="px-3 text-sm tabular-nums">
              {page} / {pagination.total_pages}
            </span>
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              disabled={page >= pagination.total_pages}
              onClick={() => setPage((p) => p + 1)}
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
