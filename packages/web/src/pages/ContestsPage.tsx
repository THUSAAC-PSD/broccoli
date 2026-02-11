import { useTranslation } from '@broccoli/sdk/i18n';
import { Trophy } from 'lucide-react';
import { Link } from 'react-router';

import { Badge } from '@/components/ui/badge';
import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import type { ServerTableParams } from '@/hooks/use-server-table';
import { api } from '@/lib/api/client';
import type { components } from '@/lib/api/schema';

type ContestListItem = components['schemas']['ContestListItem'];

// Mock data until backend is wired up
const MOCK_CONTESTS: ContestListItem[] = [
  {
    id: 1,
    title: 'Weekly Contest #1',
    is_public: true,
    start_time: '2026-02-10T14:00:00Z',
    end_time: '2026-02-15T17:00:00Z',
    show_compile_output: true,
    show_participants_list: true,
    submissions_visible: false,
    created_at: '2026-01-20T10:00:00Z',
    updated_at: '2026-01-20T10:30:00Z',
  },
  {
    id: 2,
    title: 'Monthly Challenge',
    is_public: true,
    start_time: '2026-02-01T00:00:00Z',
    end_time: '2026-03-01T23:59:00Z',
    show_compile_output: true,
    show_participants_list: true,
    submissions_visible: true,
    created_at: '2026-01-25T08:00:00Z',
    updated_at: '2026-01-25T08:30:00Z',
  },
  {
    id: 3,
    title: 'Algorithm Sprint',
    is_public: true,
    start_time: '2026-02-20T09:00:00Z',
    end_time: '2026-02-20T12:00:00Z',
    show_compile_output: false,
    show_participants_list: false,
    submissions_visible: false,
    created_at: '2026-02-01T12:00:00Z',
    updated_at: '2026-02-01T12:30:00Z',
  },
  {
    id: 4,
    title: 'Beginner Bootcamp',
    is_public: true,
    start_time: '2026-01-01T10:00:00Z',
    end_time: '2026-02-01T10:00:00Z',
    show_compile_output: true,
    show_participants_list: true,
    submissions_visible: true,
    created_at: '2025-12-15T10:00:00Z',
    updated_at: '2025-12-15T10:30:00Z',
  },
  {
    id: 5,
    title: 'Spring Championship 2026',
    is_public: false,
    start_time: '2026-03-15T08:00:00Z',
    end_time: '2026-03-15T13:00:00Z',
    show_compile_output: true,
    show_participants_list: true,
    submissions_visible: false,
    created_at: '2026-02-10T14:00:00Z',
    updated_at: '2026-02-10T14:30:00Z',
  },
];

const USE_MOCK = true;

function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' } {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now >= start && now <= end) return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

function formatRelativeTime(dateStr: string, t: (key: string, params?: Record<string, string>) => string): string {
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

async function fetchContests(params: ServerTableParams) {
  if (USE_MOCK) {
    let items = [...MOCK_CONTESTS];
    if (params.search) {
      const q = params.search.toLowerCase();
      items = items.filter((c) => c.title.toLowerCase().includes(q));
    }
    if (params.sort_by) {
      const key = params.sort_by as keyof ContestListItem;
      items.sort((a, b) => {
        const av = a[key];
        const bv = b[key];
        if (av < bv) return params.sort_order === 'asc' ? -1 : 1;
        if (av > bv) return params.sort_order === 'asc' ? 1 : -1;
        return 0;
      });
    }
    return {
      data: items,
      pagination: { page: 1, per_page: 20, total: items.length, total_pages: 1 },
    };
  }

  const { data, error } = await api.GET('/contests', {
    params: {
      path: {
        page: params.page,
        per_page: params.per_page,
        search: params.search ?? null,
        sort_by: params.sort_by ?? null,
        sort_order: params.sort_order ?? null,
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
