import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  getStatusLabel,
  type Submission,
  SUBMISSION_STATUS_FILTER_OPTIONS,
  type SubmissionStatusFilterValue,
  type SubmissionSummary,
  toSubmissionStatus,
} from '@broccoli/web-sdk/submission';
import { Badge, Button, FilterDropdown } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import {
  BookOpen,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Code2,
  Inbox,
  ListFilter,
  Search,
} from 'lucide-react';
import { useMemo, useState } from 'react';
import { Link } from 'react-router';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { fetchContestProblemList } from '@/features/contest/api/fetch-contest-problem-list';
import { useContest } from '@/features/contest/contexts/contest-context';
import { fetchSupportedLanguages } from '@/features/problem/api/fetch-supported-languages';
import { fetchContestSubmissions } from '@/features/submission/api/fetch-contest-submissions';
import { formatMemory } from '@/features/submission/components/TestCaseRow';
import { getVerdictBadge } from '@/features/submission/utils/verdict';

import { SubmissionResult } from './SubmissionResult';

const PER_PAGE = 20;

export function SubmissionsList({ contestId }: { contestId: number }) {
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

  const [expandedId, setExpandedId] = useState<number | null>(null);

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
    setExpandedId(null);
  };

  return (
    <div className="flex flex-col gap-4">
      {/* Filters */}
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
        </div>

        <Button className="h-9 md:ml-auto" onClick={applyFilters}>
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
        ) : submissions.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-muted-foreground gap-2">
            <Inbox className="h-8 w-8 opacity-30" />
            <p className="text-sm">{t('overview.noSubmissions')}</p>
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-xs font-medium text-muted-foreground bg-muted/30">
                <th className="w-8 px-4 py-2.5" />
                <th className="px-4 py-2.5 text-left font-medium">
                  {t('overview.problem')}
                </th>
                <th className="px-4 py-2.5 text-left font-medium">
                  {t('overview.verdict')}
                </th>
                <th className="px-4 py-2.5 text-left font-medium">
                  {t('overview.language')}
                </th>
                <th className="px-4 py-2.5 text-right font-medium">
                  {t('result.timeHeader')}
                </th>
                <th className="px-4 py-2.5 text-right font-medium">
                  {t('result.memoryHeader')}
                </th>
                <th className="px-4 py-2.5 text-right font-medium">
                  {t('overview.submitted')}
                </th>
                <th className="w-8 px-4 py-2.5" />
              </tr>
            </thead>
            <tbody>
              {submissions.map((sub) => (
                <SubmissionsListRow
                  key={sub.id}
                  submission={sub}
                  contestId={contestId}
                  isExpanded={expandedId === sub.id}
                  onToggle={() =>
                    setExpandedId(expandedId === sub.id ? null : sub.id)
                  }
                />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Pagination */}
      {pagination && pagination.total_pages > 1 && (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            {t('overview.submitted')} {(page - 1) * PER_PAGE + 1}–
            {Math.min(page * PER_PAGE, pagination.total)} / {pagination.total}
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

function SubmissionsListRow({
  submission,
  contestId,
  isExpanded,
  onToggle,
}: {
  submission: SubmissionSummary;
  contestId: number;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const { label: verdictLabel, variant: verdictVariant } = getVerdictBadge(
    submission.verdict ?? null,
    submission.status,
    t,
  );

  return (
    <>
      <tr
        onClick={onToggle}
        className={`border-b last:border-b-0 cursor-pointer transition-colors duration-75 hover:bg-muted/30 ${
          isExpanded ? 'bg-muted/20' : ''
        }`}
      >
        <td className="px-4 py-2.5 text-muted-foreground/50">
          {isExpanded ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
        </td>
        <td className="px-4 py-2.5 font-medium">
          <span className="truncate">
            {submission.problem_title}
            <span className="ml-1.5 text-xs font-mono text-muted-foreground/40">
              #{submission.id}
            </span>
          </span>
        </td>
        <td className="px-4 py-2.5">
          {verdictLabel ? (
            <Badge variant={verdictVariant} className="text-xs">
              {verdictLabel}
            </Badge>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </td>
        <td className="px-4 py-2.5 text-xs font-mono text-muted-foreground">
          {submission.language}
        </td>
        <td className="px-4 py-2.5 text-xs font-mono text-muted-foreground text-right tabular-nums whitespace-nowrap">
          {submission.time_used != null ? `${submission.time_used}ms` : '—'}
        </td>
        <td className="px-4 py-2.5 text-xs font-mono text-muted-foreground text-right tabular-nums whitespace-nowrap">
          {submission.memory_used != null
            ? `${formatMemory(submission.memory_used)}MB`
            : '—'}
        </td>
        <td className="px-4 py-2.5 text-xs text-muted-foreground text-right whitespace-nowrap">
          {formatRelativeDatetime(submission.created_at, t)}
        </td>
        <td className="px-4 py-2.5" onClick={(e) => e.stopPropagation()}>
          <Link
            to={`/contests/${contestId}/submissions/${submission.id}`}
            className="text-muted-foreground/50 hover:text-primary transition-colors"
            title={t('submissions.viewDetails')}
          >
            <BookOpen className="h-3.5 w-3.5" />
          </Link>
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td colSpan={8} className="px-4 pb-4 pt-1 border-b">
            <SubmissionDetail submissionId={submission.id} />
          </td>
        </tr>
      )}
    </>
  );
}

function SubmissionDetail({ submissionId }: { submissionId: number }) {
  const apiClient = useApiClient();

  const { data: submission, isLoading } = useQuery<Submission>({
    queryKey: ['submission', submissionId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions/{id}', {
        params: { path: { id: submissionId } },
      });
      if (error) throw error;
      return data;
    },
    staleTime: 60_000,
  });

  if (isLoading || !submission) {
    return (
      <div className="flex items-center justify-center py-6">
        <span className="h-4 w-4 rounded-full border-2 border-primary border-t-transparent animate-spin" />
      </div>
    );
  }

  return <SubmissionResult submission={submission} />;
}
