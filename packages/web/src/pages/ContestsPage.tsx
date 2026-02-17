import type { ContestListItem } from '@broccoli/sdk';
import type { ApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Trophy } from 'lucide-react';
import { Link } from 'react-router';

import { Badge } from '@/components/ui/badge';
import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import type { ServerTableParams } from '@/hooks/use-server-table';

function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): {
  label: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
} {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now >= start && now <= end)
    return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

function formatRelativeTime(
  dateStr: string,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = date.getTime() - now.getTime();
  const absDiffMs = Math.abs(diffMs);
  const mins = Math.floor(absDiffMs / 60000);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);

  if (diffMs > 0) {
    if (days > 0) return t('contests.inDays', { count: String(days) });
    if (hours > 0) return t('contests.inHours', { count: String(hours) });
    return t('contests.inMinutes', { count: String(mins) });
  }
  if (days > 0) return t('contests.daysAgo', { count: String(days) });
  if (hours > 0) return t('contests.hoursAgo', { count: String(hours) });
  return t('contests.minutesAgo', { count: String(mins) });
}

async function fetchContests(apiClient: ApiClient, params: ServerTableParams) {
  const { data, error } = await apiClient.GET('/contests', {
    params: {
      query: {
        page: params.page,
        per_page: params.per_page,
        search: params.search,
        sort_by: params.sort_by,
        sort_order: params.sort_order,
      },
    },
  });
  if (error) throw error;
  return {
    data: data.data,
    pagination: data.pagination,
  };
}

function useContestsColumns(): DataTableColumn<ContestListItem>[] {
  const { t } = useTranslation();

  return [
    {
      accessorKey: 'title',
      header: t('contests.titleColumn'),
      sortKey: 'title',
      cell: ({ row }) => (
        <Link
          to={`/contests/${row.original.id}`}
          className="font-medium hover:text-primary hover:underline"
        >
          {row.original.title}
        </Link>
      ),
    },
    {
      id: 'status',
      header: t('contests.status'),
      size: 120,
      cell: ({ row }) => {
        const { label, variant } = getContestStatus(
          row.original.start_time,
          row.original.end_time,
          t,
        );
        return <Badge variant={variant}>{label}</Badge>;
      },
    },
    {
      accessorKey: 'start_time',
      header: t('contests.startTime'),
      size: 160,
      sortKey: 'start_time',
      cell: ({ row }) => formatRelativeTime(row.original.start_time, t),
    },
    {
      accessorKey: 'end_time',
      header: t('contests.endTime'),
      size: 160,
      cell: ({ row }) => formatRelativeTime(row.original.end_time, t),
    },
  ];
}

export function ContestsPage() {
  const { t } = useTranslation();
  const columns = useContestsColumns();

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Trophy className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{t('contests.title')}</h1>
      </div>

      <DataTable
        columns={columns}
        queryKey={['contests']}
        fetchFn={fetchContests}
        searchable
        searchPlaceholder={t('contests.searchPlaceholder')}
        defaultPerPage={20}
        defaultSortBy="start_time"
        defaultSortOrder="desc"
        emptyMessage={t('contests.empty')}
      />
    </div>
  );
}
