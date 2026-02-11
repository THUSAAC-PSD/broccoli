import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Code2 } from 'lucide-react';
import { Link, useNavigate } from 'react-router';

import { Badge } from '@/components/ui/badge';
import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import type { ServerTableParams } from '@/hooks/use-server-table';
import { api } from '@/lib/api/client';
import type { components } from '@/lib/api/schema';

type ProblemListItem = components['schemas']['ProblemListItem'];
type ContestProblemResponse = components['schemas']['ContestProblemResponse'];

// Mock data until backend is wired up
const MOCK_PROBLEMS: ProblemListItem[] = [
  {
    id: 1,
    title: 'Two Sum',
    time_limit: 1000,
    memory_limit: 262144,
    show_test_details: false,
    created_at: '2025-09-01T08:00:00Z',
    updated_at: '2025-09-01T08:30:00Z',
  },
  {
    id: 2,
    title: 'Add Two Numbers',
    time_limit: 2000,
    memory_limit: 262144,
    show_test_details: false,
    created_at: '2025-09-02T10:00:00Z',
    updated_at: '2025-09-02T10:30:00Z',
  },
  {
    id: 3,
    title: 'Longest Substring Without Repeating Characters',
    time_limit: 1000,
    memory_limit: 262144,
    show_test_details: true,
    created_at: '2025-09-03T12:00:00Z',
    updated_at: '2025-09-03T12:30:00Z',
  },
  {
    id: 4,
    title: 'Median of Two Sorted Arrays',
    time_limit: 2000,
    memory_limit: 524288,
    show_test_details: false,
    created_at: '2025-09-04T09:00:00Z',
    updated_at: '2025-09-04T09:30:00Z',
  },
  {
    id: 5,
    title: 'Longest Palindromic Substring',
    time_limit: 1000,
    memory_limit: 262144,
    show_test_details: true,
    created_at: '2025-09-05T14:00:00Z',
    updated_at: '2025-09-05T14:30:00Z',
  },
];

// Mock contest mapping: problem_id -> contest info with end_time as due date
const MOCK_CONTEST_MAP: Record<
  number,
  { id: number; name: string; endTime: string }
> = {
  1: { id: 1, name: 'Weekly Contest #1', endTime: '2026-02-15T17:00:00Z' },
  2: { id: 1, name: 'Weekly Contest #1', endTime: '2026-02-15T17:00:00Z' },
  3: { id: 2, name: 'Monthly Challenge', endTime: '2026-03-01T23:59:00Z' },
  4: { id: 2, name: 'Monthly Challenge', endTime: '2026-03-01T23:59:00Z' },
};

const USE_MOCK = true;

// --- Public problems (paginated via DataTable) ---

async function fetchProblems(params: ServerTableParams) {
  if (USE_MOCK) {
    let items = [...MOCK_PROBLEMS];
    if (params.search) {
      const q = params.search.toLowerCase();
      items = items.filter((p) => p.title.toLowerCase().includes(q));
    }
    if (params.sort_by) {
      const key = params.sort_by as keyof ProblemListItem;
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

  const { data, error } = await api.GET('/problems', {
    params: {
      query: {
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

function useProblemsColumns(): DataTableColumn<ProblemListItem>[] {
  const { t } = useTranslation();

  return [
    {
      accessorKey: 'id',
      header: '#',
      size: 80,
      sortKey: 'id',
    },
    {
      accessorKey: 'title',
      header: t('problems.titleColumn'),
      sortKey: 'title',
      cell: ({ row }) => (
        <Link
          to={`/problems/${row.original.id}`}
          className="font-medium hover:text-primary hover:underline"
        >
          {row.original.title}
        </Link>
      ),
    },
    {
      id: 'contest',
      header: t('problems.contest'),
      cell: ({ row }) => {
        const contest = MOCK_CONTEST_MAP[row.original.id];
        if (!contest) return <span className="text-muted-foreground">—</span>;
        return (
          <Link to={`/contests/${contest.id}`}>
            <Badge variant="secondary">{contest.name}</Badge>
          </Link>
        );
      },
    },
    {
      id: 'due',
      header: t('problems.due'),
      size: 160,
      cell: ({ row }) => {
        const contest = MOCK_CONTEST_MAP[row.original.id];
        if (!contest) return <span className="text-muted-foreground">—</span>;
        const due = new Date(contest.endTime);
        const now = new Date();
        const diffMs = due.getTime() - now.getTime();
        if (diffMs <= 0) {
          return <span className="text-muted-foreground">{t('problems.dueEnded')}</span>;
        }
        const diffMins = Math.floor(diffMs / 60000);
        const diffHours = Math.floor(diffMins / 60);
        const diffDays = Math.floor(diffHours / 24);
        let label: string;
        if (diffDays > 0) {
          label = t('problems.dueInDays', { count: String(diffDays) });
        } else if (diffHours > 0) {
          label = t('problems.dueInHours', { count: String(diffHours) });
        } else {
          label = t('problems.dueInMinutes', { count: String(diffMins) });
        }
        return <span>{label}</span>;
      },
    },
  ];
}

// --- Contest problems (simple table via React Query) ---

function ContestProblemsTable({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const { data: problems = [], isLoading } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: async () => {
      const { data, error } = await api.GET('/contests/{id}/problems', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data;
    },
  });

  if (isLoading) {
    return (
      <div className="flex justify-center py-12 text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (problems.length === 0) {
    return (
      <div className="flex justify-center py-12 text-muted-foreground">
        {t('problems.empty')}
      </div>
    );
  }

  return (
    <div className="rounded-md border">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b bg-muted/50">
            <th className="px-4 py-3 text-left font-medium w-20">
              {t('problems.label')}
            </th>
            <th className="px-4 py-3 text-left font-medium">
              {t('problems.titleColumn')}
            </th>
          </tr>
        </thead>
        <tbody>
          {problems.map((p: ContestProblemResponse) => (
            <tr
              key={p.problem_id}
              className="border-b cursor-pointer hover:bg-muted/50 transition-colors"
              onClick={() =>
                navigate(`/contests/${contestId}/problems/${p.problem_id}`)
              }
            >
              <td className="px-4 py-3 font-semibold">{p.label}</td>
              <td className="px-4 py-3">{p.problem_title}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// --- Page ---

export function ProblemsPage({ contestId }: { contestId?: number }) {
  const { t } = useTranslation();
  const columns = useProblemsColumns();

  const title = contestId ? t('problems.contestProblems') : t('problems.title');

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Code2 className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{title}</h1>
      </div>

      {contestId ? (
        <ContestProblemsTable contestId={contestId} />
      ) : (
        <DataTable
          columns={columns}
          queryKey={['problems']}
          fetchFn={fetchProblems}
          searchable
          searchPlaceholder={t('problems.searchPlaceholder')}
          defaultPerPage={20}
          defaultSortBy="created_at"
          defaultSortOrder="desc"
          emptyMessage={t('problems.empty')}
        />
      )}
    </div>
  );
}
