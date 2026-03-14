import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { SubmissionSummary } from '@broccoli/web-sdk/submission';
import { Badge, Button, type DataTableColumn } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { FileText } from 'lucide-react';
import { Link } from 'react-router';

import { getVerdictBadge } from '@/features/submission/utils/verdict';

export function useSubmissionColumns(
  contestId: number,
): DataTableColumn<SubmissionSummary>[] {
  const { t } = useTranslation();

  return [
    {
      accessorKey: 'problem_title',
      header: t('overview.problem'),
      cell: ({ row }) => (
        <Link
          to={`/contests/${contestId}/problems/${row.original.problem_id}`}
          className="font-medium hover:text-primary hover:underline"
        >
          {row.original.problem_title}
        </Link>
      ),
    },
    {
      accessorKey: 'language',
      header: t('overview.language'),
      cell: ({ row }) => (
        <Badge variant="outline">{row.original.language}</Badge>
      ),
    },
    {
      accessorKey: 'verdict',
      header: t('contests.status'),
      cell: ({ row }) => {
        const vb = getVerdictBadge(
          row.original.verdict,
          row.original.status,
          t,
        );
        return <Badge variant={vb.variant}>{vb.label}</Badge>;
      },
    },
    {
      accessorKey: 'created_at',
      header: t('overview.submitted'),
      sortKey: 'created_at',
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {formatRelativeDatetime(row.original.created_at, t)}
        </span>
      ),
    },
    {
      id: 'actions',
      header: t('submissions.details'),
      size: 60,
      cell: ({ row }) => (
        <div className="flex justify-center">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            asChild
            aria-label={t('submissions.viewDetails')}
            title={t('submissions.viewDetails')}
          >
            <Link to={`/contests/${contestId}/submissions/${row.original.id}`}>
              <FileText className="h-4 w-4" />
            </Link>
          </Button>
        </div>
      ),
    },
  ];
}
