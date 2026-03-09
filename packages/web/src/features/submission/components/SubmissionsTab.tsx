import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  getStatusLabel,
  SUBMISSION_STATUS_FILTER_OPTIONS,
  type SubmissionStatusFilterValue,
  toSubmissionStatus,
} from '@broccoli/web-sdk/submission';
import { Button, DataTable, FilterDropdown } from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import { BookOpen, Code2, ListFilter, Search } from 'lucide-react';
import { useMemo, useState } from 'react';

import { ListSkeleton } from '@/components/ListSkeleton';
import { fetchContestProblemList } from '@/features/contest/api/fetch-contest-problem-list';
import { useContest } from '@/features/contest/contexts/contest-context';
import { fetchContestSubmissions } from '@/features/submission/api/fetch-contest-submissions';
import { useSubmissionColumns } from '@/features/submission/hooks/use-submission-columns';

export function SubmissionsTab({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { filterProblemId, setFilterProblemId } = useContest();
  const columns = useSubmissionColumns(contestId);
  const [draftProblemId, setDraftProblemId] = useState<string>(
    filterProblemId ? String(filterProblemId) : 'all',
  );
  const [draftLanguage, setDraftLanguage] = useState<string>('all');
  const [draftStatus, setDraftStatus] =
    useState<SubmissionStatusFilterValue>('all');
  const [appliedLanguage, setAppliedLanguage] = useState<string | null>(null);
  const [appliedStatus, setAppliedStatus] =
    useState<SubmissionStatusFilterValue>('all');

  const {
    data: problems = [],
    isLoading: isProblemsLoading,
    error: problemsError,
  } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: () => fetchContestProblemList(apiClient, contestId),
  });

  const { data: languageSeed } = useQuery({
    queryKey: ['contest-submission-languages', contestId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET(
        '/contests/{id}/submissions',
        {
          params: {
            path: { id: contestId },
            query: {
              page: 1,
              per_page: 100,
              sort_by: 'created_at',
              sort_order: 'desc',
            },
          },
        },
      );
      if (error) throw error;
      return data.data;
    },
  });

  const languageOptions = useMemo(() => {
    const set = new Set<string>();
    (languageSeed ?? []).forEach((s) => {
      if (s.language?.trim()) set.add(s.language.trim());
    });
    return ['all', ...Array.from(set).sort((a, b) => a.localeCompare(b))];
  }, [languageSeed]);

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

  const statusOptions = useMemo(
    () =>
      SUBMISSION_STATUS_FILTER_OPTIONS.map((status) => ({
        value: status,
        label: getStatusLabel(status, t),
      })),
    [t],
  );

  const languageDropdownOptions = useMemo(
    () => [
      { value: 'all', label: t('submissions.filters.allLanguages') },
      ...languageOptions
        .filter((v) => v !== 'all')
        .map((lang) => ({ value: lang, label: lang })),
    ],
    [languageOptions, t],
  );

  if (isProblemsLoading) {
    return <ListSkeleton />;
  }

  if (problemsError) {
    return (
      <div className="text-sm text-destructive">{t('contests.loadError')}</div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
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
            options={languageDropdownOptions}
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
        </div>

        <Button
          className="h-9 md:ml-auto"
          onClick={() => {
            const nextProblemId =
              draftProblemId === 'all' ? null : Number(draftProblemId);
            setFilterProblemId(nextProblemId);
            setAppliedLanguage(draftLanguage === 'all' ? null : draftLanguage);
            setAppliedStatus(draftStatus);
          }}
        >
          <Search className="mr-1 h-4 w-4" />
          {t('submissions.filters.search')}
        </Button>
      </div>

      <DataTable
        key={`${contestId}-${filterProblemId ?? 'all'}-${appliedLanguage ?? 'all'}-${appliedStatus}`}
        columns={columns}
        queryKey={[
          'contest-submissions-table',
          String(contestId),
          String(filterProblemId ?? 'all'),
          String(appliedLanguage ?? 'all'),
          appliedStatus,
        ]}
        fetchFn={(api, params) =>
          fetchContestSubmissions(api, {
            ...params,
            contestId,
            problemId: filterProblemId,
            language: appliedLanguage,
            status: toSubmissionStatus(appliedStatus),
          })
        }
        defaultPerPage={20}
        defaultSortBy="created_at"
        defaultSortOrder="desc"
        emptyMessage={t('overview.noSubmissions')}
      />
    </div>
  );
}
