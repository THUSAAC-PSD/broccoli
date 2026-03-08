import type { SubmissionListItem } from '@broccoli/web-sdk';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Link } from 'react-router';

import { Badge } from '@/components/ui/badge';
import type { DataTableColumn } from '@/components/ui/data-table';
import { getVerdictBadge } from '@/features/submission/utils/verdict';
import { formatRelativeDatetime } from '@/lib/utils';

export function useSubmissionColumns(
  contestId: number,
): DataTableColumn<SubmissionListItem>[] {
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
        const vb = getVerdictBadge(row.original.verdict, row.original.status);
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
  ];
}
