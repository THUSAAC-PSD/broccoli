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
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { fetchContestProblemList } from '@/features/contest/api/fetch-contest-problem-list';
import { fetchSupportedLanguages } from '@/features/problem/api/fetch-supported-languages';
import { fetchContestSubmissions } from '@/features/submission/api/fetch-contest-submissions';

import { SubmissionsTable } from './SubmissionsTable';

const PER_PAGE = 20;

function parsePositiveInt(value: string | null) {
  if (!value) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

function parseStatus(value: string | null): SubmissionStatusFilterValue {
  return SUBMISSION_STATUS_FILTER_OPTIONS.includes(
    value as SubmissionStatusFilterValue,
  )
    ? (value as SubmissionStatusFilterValue)
    : 'all';
}

export function ContestSubmissions({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const [searchParams, setSearchParams] = useSearchParams();

  const page = parsePositiveInt(searchParams.get('page')) ?? 1;
  const appliedProblemId = parsePositiveInt(searchParams.get('problem'));
  const appliedLanguage = searchParams.get('language') ?? null;
  const appliedStatus = parseStatus(searchParams.get('status'));

  const [draftProblemId, setDraftProblemId] = useState<string>(
    appliedProblemId ? String(appliedProblemId) : 'all',
  );
  const [draftLanguage, setDraftLanguage] = useState<string>(
    appliedLanguage ?? 'all',
  );
  const [draftStatus, setDraftStatus] =
    useState<SubmissionStatusFilterValue>(appliedStatus);

  useEffect(() => {
    setDraftProblemId(appliedProblemId ? String(appliedProblemId) : 'all');
    setDraftLanguage(appliedLanguage ?? 'all');
    setDraftStatus(appliedStatus);
  }, [appliedLanguage, appliedProblemId, appliedStatus]);

  const updateSearchParams = useCallback(
    (
      updates: {
        page?: number;
        problem?: number | null;
        language?: string | null;
        status?: SubmissionStatusFilterValue;
      },
      options?: { replace?: boolean },
    ) => {
      const next = new URLSearchParams(searchParams);
      const nextPage = updates.page ?? page;
      const nextProblem =
        updates.problem === undefined ? appliedProblemId : updates.problem;
      const nextLanguage =
        updates.language === undefined ? appliedLanguage : updates.language;
      const nextStatus = updates.status ?? appliedStatus;

      if (nextPage <= 1) {
        next.delete('page');
      } else {
        next.set('page', String(nextPage));
      }

      if (nextProblem) {
        next.set('problem', String(nextProblem));
      } else {
        next.delete('problem');
      }

      if (nextLanguage) {
        next.set('language', nextLanguage);
      } else {
        next.delete('language');
      }

      if (nextStatus !== 'all') {
        next.set('status', nextStatus);
      } else {
        next.delete('status');
      }

      setSearchParams(next, { replace: options?.replace ?? false });
    },
    [
      appliedLanguage,
      appliedProblemId,
      appliedStatus,
      page,
      searchParams,
      setSearchParams,
    ],
  );

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
      String(appliedProblemId ?? 'all'),
      String(appliedLanguage ?? 'all'),
      appliedStatus,
      page,
    ],
    enabled: !!user,
    queryFn: () =>
      fetchContestSubmissions(apiClient, {
        contestId,
        problemId: appliedProblemId,
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
    updateSearchParams({
      page: 1,
      problem: nextProblemId,
      language: draftLanguage === 'all' ? null : draftLanguage,
      status: draftStatus,
    });
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
              onClick={() => updateSearchParams({ page: page - 1 })}
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
              onClick={() => updateSearchParams({ page: page + 1 })}
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
