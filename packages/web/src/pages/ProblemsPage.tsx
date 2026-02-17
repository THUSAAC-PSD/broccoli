import type { ProblemListItem, ContestProblemResponse } from '@broccoli/sdk';
import { useApiClient, type ApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Code2 } from 'lucide-react';
import { Link, useNavigate } from 'react-router';

import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import type { ServerTableParams } from '@/hooks/use-server-table';

// --- Public problems (paginated via DataTable) ---

async function fetchProblems(apiClient: ApiClient, params: ServerTableParams) {
  const { data, error } = await apiClient.GET('/problems', {
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
      cell: () => {
        return <span className="text-muted-foreground">—</span>;
      },
    },
    {
      id: 'due',
      header: t('problems.due'),
      size: 160,
      cell: () => {
        return <span className="text-muted-foreground">—</span>;
      },
    },
  ];
}

// --- Contest problems (simple table via React Query) ---

function ContestProblemsTable({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const apiClient = useApiClient();

  const { data: problems = [], isLoading } = useQuery({
    queryKey: ['contest-problems', contestId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
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
