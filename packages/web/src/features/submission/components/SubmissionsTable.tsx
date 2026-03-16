import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type {
  Submission,
  SubmissionSummary,
} from '@broccoli/web-sdk/submission';
import { Badge } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, ExternalLink, Inbox } from 'lucide-react';
import { useRef, useState } from 'react';
import { Link } from 'react-router';

import { getVerdictBadge } from '@/features/submission/utils/verdict';

import { SubmissionResult } from './SubmissionResult';
import { formatMemory } from './TestCaseRow';

export interface SubmissionsTableColumn {
  key: 'problem' | 'verdict' | 'language' | 'time' | 'memory' | 'submitted';
  align?: 'left' | 'right';
}

const DEFAULT_COLUMNS: SubmissionsTableColumn[] = [
  { key: 'verdict', align: 'left' },
  { key: 'language', align: 'right' },
  { key: 'time', align: 'right' },
  { key: 'memory', align: 'right' },
  { key: 'submitted', align: 'right' },
];

const FULL_COLUMNS: SubmissionsTableColumn[] = [
  { key: 'problem', align: 'left' },
  { key: 'verdict', align: 'left' },
  { key: 'language', align: 'left' },
  { key: 'time', align: 'right' },
  { key: 'memory', align: 'right' },
  { key: 'submitted', align: 'right' },
];

const COLUMN_HEADERS: Record<SubmissionsTableColumn['key'], string> = {
  problem: 'overview.problem',
  verdict: 'overview.verdict',
  language: 'overview.language',
  time: 'result.timeHeader',
  memory: 'result.memoryHeader',
  submitted: 'overview.submitted',
};

export interface SubmissionsTableProps {
  submissions: SubmissionSummary[];
  /** Which columns to show. Defaults to verdict/language/time/memory/submitted. */
  columns?: SubmissionsTableColumn[];
  /** If true, rows are expandable and show full submission detail. Default true. */
  expandable?: boolean;
  /** Builds a link URL for the #id badge. If omitted, #id is plain text. */
  linkBuilder?: (submission: SubmissionSummary) => string;
  /** Render custom verdict node instead of default badge (used by session entries). */
  renderVerdict?: (submission: SubmissionSummary) => React.ReactNode;
  /** Additional class on the table wrapper. */
  className?: string;
  /** Compact mode: smaller text, tighter padding. */
  compact?: boolean;
  /** Sticky header for scrollable containers. */
  stickyHeader?: boolean;
  /** Message when empty. Defaults to 'No submissions yet'. */
  emptyMessage?: string;
  /** Custom row highlight. */
  rowClassName?: (submission: SubmissionSummary) => string;
  /** Submission ID to auto-expand on mount / when changed. */
  autoExpandId?: number | null;
  /** Custom expanded detail renderer. Return undefined to use default (API fetch). */
  renderExpandedDetail?: (
    submission: SubmissionSummary,
  ) => React.ReactNode | undefined;
}

export function SubmissionsTable({
  submissions,
  columns,
  expandable = true,
  linkBuilder,
  renderVerdict,
  className,
  compact = false,
  stickyHeader = false,
  emptyMessage,
  rowClassName,
  autoExpandId,
  renderExpandedDetail,
}: SubmissionsTableProps) {
  const { t } = useTranslation();
  const [expandedId, setExpandedId] = useState<number | null>(
    autoExpandId ?? null,
  );

  // Track autoExpandId changes
  const prevAutoExpandRef = useRef(autoExpandId);
  if (autoExpandId !== prevAutoExpandRef.current) {
    prevAutoExpandRef.current = autoExpandId;
    if (autoExpandId != null) {
      setExpandedId(autoExpandId);
    }
  }
  const cols = columns ?? DEFAULT_COLUMNS;
  const colSpan = cols.length + (expandable ? 1 : 0);

  const px = compact ? 'px-2' : 'px-4';
  const py = compact ? 'py-1.5' : 'py-2.5';
  const textSize = compact ? 'text-[11px]' : 'text-sm';
  const headerTextSize = compact ? 'text-[10px]' : 'text-xs';

  if (submissions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground gap-2">
        <Inbox className="h-7 w-7 opacity-30" />
        <p className="text-sm">{emptyMessage ?? t('overview.noSubmissions')}</p>
      </div>
    );
  }

  return (
    <table className={`w-full ${textSize} ${className ?? ''}`}>
      <thead className={stickyHeader ? 'sticky top-0 z-10' : ''}>
        <tr
          className={`border-b ${headerTextSize} font-medium text-muted-foreground ${
            stickyHeader ? 'bg-muted/60 backdrop-blur-sm' : 'bg-muted/30'
          }`}
        >
          {expandable && <th className={`w-5 ${px} ${py}`} />}
          {cols.map((col) => (
            <th
              key={col.key}
              className={`${px} ${py} font-medium ${
                col.align === 'right' ? 'text-right' : 'text-left'
              } ${col.key === cols[cols.length - 1]?.key && !compact ? 'pr-4' : ''}`}
            >
              {t(COLUMN_HEADERS[col.key])}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {submissions.map((sub) => (
          <SubmissionRow
            key={sub.id}
            submission={sub}
            columns={cols}
            expandable={expandable}
            isExpanded={expandedId === sub.id}
            onToggle={() =>
              setExpandedId(expandedId === sub.id ? null : sub.id)
            }
            linkBuilder={linkBuilder}
            renderVerdict={renderVerdict}
            renderExpandedDetail={renderExpandedDetail}
            compact={compact}
            colSpan={colSpan}
            rowClassName={rowClassName?.(sub)}
          />
        ))}
      </tbody>
    </table>
  );
}

/** Columns for the full submissions page (includes problem title). */
SubmissionsTable.fullColumns = FULL_COLUMNS;

/** Columns for compact/panel view (no problem title). */
SubmissionsTable.compactColumns = DEFAULT_COLUMNS;

function SubmissionRow({
  submission,
  columns,
  expandable,
  isExpanded,
  onToggle,
  linkBuilder,
  renderVerdict,
  renderExpandedDetail,
  compact,
  colSpan,
  rowClassName,
}: {
  submission: SubmissionSummary;
  columns: SubmissionsTableColumn[];
  expandable: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  linkBuilder?: (submission: SubmissionSummary) => string;
  renderVerdict?: (submission: SubmissionSummary) => React.ReactNode;
  renderExpandedDetail?: (
    submission: SubmissionSummary,
  ) => React.ReactNode | undefined;
  compact: boolean;
  colSpan: number;
  rowClassName?: string;
}) {
  const { t } = useTranslation();
  const { label: verdictLabel, variant: verdictVariant } = getVerdictBadge(
    submission.verdict ?? null,
    submission.status,
    t,
  );

  const px = compact ? 'px-2' : 'px-4';
  const py = compact ? 'py-2' : 'py-2.5';
  const badgeSize = compact ? 'text-[10px] px-1.5 py-0 h-4' : 'text-xs';

  const cellRenderers: Record<
    SubmissionsTableColumn['key'],
    () => React.ReactNode
  > = {
    problem: () => (
      <td key="problem" className={`${px} ${py}`}>
        <div className="flex items-center gap-2">
          <span className="font-medium">{submission.problem_title}</span>
          {linkBuilder ? (
            <Link
              to={linkBuilder(submission)}
              onClick={(e) => e.stopPropagation()}
              className="inline-flex items-center gap-0.5 text-xs font-mono text-primary/60 hover:text-primary transition-colors group"
              title={t('submissions.viewDetails')}
            >
              #{submission.id}
              <ExternalLink className="h-2.5 w-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
            </Link>
          ) : (
            <span className="text-[10px] font-mono text-muted-foreground/40">
              #{submission.id}
            </span>
          )}
        </div>
      </td>
    ),
    verdict: () => {
      const customVerdict = renderVerdict?.(submission);
      const defaultVerdict = verdictLabel ? (
        <Badge variant={verdictVariant} className={badgeSize}>
          {verdictLabel}
        </Badge>
      ) : (
        <span className="text-muted-foreground">—</span>
      );
      return (
        <td key="verdict" className={`${px} ${py}`}>
          <span className="flex items-center gap-1.5">
            {customVerdict ?? defaultVerdict}
            {/* Show #id next to verdict when there's no problem column */}
            {!columns.some((c) => c.key === 'problem') && (
              <>
                {linkBuilder ? (
                  <Link
                    to={linkBuilder(submission)}
                    onClick={(e) => e.stopPropagation()}
                    className="inline-flex items-center gap-0.5 text-[10px] font-mono text-primary/60 hover:text-primary transition-colors group"
                  >
                    #{submission.id}
                    <ExternalLink className="h-2.5 w-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
                  </Link>
                ) : (
                  <span className="text-[10px] font-mono text-muted-foreground/40">
                    #{submission.id}
                  </span>
                )}
              </>
            )}
          </span>
        </td>
      );
    },
    language: () => (
      <td
        key="language"
        className={`${px} ${py} font-mono text-muted-foreground/60 whitespace-nowrap ${
          columns.find((c) => c.key === 'language')?.align === 'right'
            ? 'text-right'
            : ''
        }`}
      >
        {submission.language}
      </td>
    ),
    time: () => (
      <td
        key="time"
        className={`${px} ${py} font-mono text-muted-foreground/60 text-right whitespace-nowrap tabular-nums`}
      >
        {submission.time_used != null ? `${submission.time_used}ms` : '—'}
      </td>
    ),
    memory: () => (
      <td
        key="memory"
        className={`${px} ${py} font-mono text-muted-foreground/60 text-right whitespace-nowrap tabular-nums`}
      >
        {submission.memory_used != null
          ? `${formatMemory(submission.memory_used)}MB`
          : '—'}
      </td>
    ),
    submitted: () => (
      <td
        key="submitted"
        className={`${px} ${py} text-muted-foreground/50 text-right whitespace-nowrap ${compact ? 'pr-3' : ''}`}
      >
        {formatRelativeDatetime(submission.created_at, t)}
      </td>
    ),
  };

  return (
    <>
      <tr
        onClick={expandable ? onToggle : undefined}
        className={`border-b last:border-b-0 transition-colors duration-75 ${
          expandable ? 'cursor-pointer hover:bg-muted/30' : 'hover:bg-muted/20'
        } ${isExpanded ? 'bg-muted/20' : ''} ${rowClassName ?? ''}`}
      >
        {expandable && (
          <td className={`${px} ${py} text-muted-foreground/50`}>
            {isExpanded ? (
              <ChevronDown className={compact ? 'h-3.5 w-3.5' : 'h-4 w-4'} />
            ) : (
              <ChevronRight className={compact ? 'h-3.5 w-3.5' : 'h-4 w-4'} />
            )}
          </td>
        )}
        {columns.map((col) => cellRenderers[col.key]())}
      </tr>
      {expandable && isExpanded && (
        <tr>
          <td
            colSpan={colSpan}
            className={`${compact ? 'px-3 pb-3 pt-0.5' : 'px-4 pb-4 pt-1'} border-b`}
          >
            {renderExpandedDetail?.(submission) ?? (
              <ExpandedDetail submissionId={submission.id} />
            )}
          </td>
        </tr>
      )}
    </>
  );
}

function ExpandedDetail({ submissionId }: { submissionId: number }) {
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
      <div className="flex items-center justify-center py-4">
        <span className="h-4 w-4 rounded-full border-2 border-primary border-t-transparent animate-spin" />
      </div>
    );
  }

  return <SubmissionResult submission={submission} />;
}
