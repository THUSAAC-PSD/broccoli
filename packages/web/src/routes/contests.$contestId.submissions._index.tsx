import { SUBMISSION_STATUSES, type SubmissionStatus } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import {
  BookOpen,
  Check,
  ChevronDown,
  Code2,
  ListFilter,
  Search,
} from 'lucide-react';
import { type ReactNode, useMemo, useState } from 'react';
import { useParams } from 'react-router';

import { ListSkeleton } from '@/components/ListSkeleton';
import { PageLayout } from '@/components/PageLayout';
import { Button } from '@/components/ui/button';
import { DataTable } from '@/components/ui/data-table';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { fetchContestProblemList } from '@/features/contest/api/fetch-contest-problem-list';
import { useContest } from '@/features/contest/contexts/contest-context';
import { useContestInfo } from '@/features/contest/hooks/use-contest-info';
import { fetchContestSubmissions } from '@/features/submission/api/fetch-contest-submissions';
import { useSubmissionColumns } from '@/features/submission/hooks/use-submission-columns';

type SubmissionStatusFilterValue = 'all' | SubmissionStatus;

const SUBMISSION_STATUS_FILTER_OPTIONS: SubmissionStatusFilterValue[] = [
  'all',
  ...SUBMISSION_STATUSES,
];

function getStatusLabel(
  status: SubmissionStatusFilterValue,
  t: (key: string) => string,
) {
  if (status === 'all') return t('submissions.filters.allStatuses');
  if (status === 'Pending') return t('result.pending');
  if (status === 'Compiling') return t('result.compilingShort');
  if (status === 'Running') return t('result.runningShort');
  if (status === 'Judged') return t('result.judged');
  if (status === 'CompilationError') return t('result.compilationError');
  return t('result.systemError');
}

function FilterDropdown({
  icon,
  value,
  options,
  onChange,
  className,
}: {
  icon: ReactNode;
  value: string;
  options: Array<{ value: string; label: string }>;
  onChange: (next: string) => void;
  className?: string;
}) {
  const selectedLabel =
    options.find((option) => option.value === value)?.label ??
    options[0]?.label;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          className={`h-9 justify-between gap-2 ${className ?? ''}`}
        >
          <span className="flex min-w-0 items-center gap-2 truncate">
            <span className="text-muted-foreground">{icon}</span>
            <span className="truncate">{selectedLabel}</span>
          </span>
          <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-72">
        {options.map((option) => (
          <DropdownMenuItem
            key={option.value}
            onClick={() => onChange(option.value)}
            className="flex items-center justify-between gap-2"
          >
            <span className="truncate">{option.label}</span>
            {option.value === value ? <Check className="h-4 w-4" /> : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function toSubmissionStatus(
  value: SubmissionStatusFilterValue,
): SubmissionStatus | undefined {
  return value === 'all' ? undefined : value;
}

function SubmissionsTab({ contestId }: { contestId: number }) {
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

export default function ContestSubmissionsPage() {
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
      pageId="contest-submissions"
      title={t('sidebar.submissions')}
      subtitle={contest?.title}
      icon={<Code2 className="h-6 w-6 text-primary" />}
    >
      <SubmissionsTab contestId={id} />
    </PageLayout>
  );
}
