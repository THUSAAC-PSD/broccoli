import type { ControlledServerTableState } from '@broccoli/web-sdk/hooks';
import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router';

interface UseTableSearchParamsOptions {
  defaultSortBy?: string;
  defaultSortOrder?: 'asc' | 'desc';
}

function parsePositiveInt(value: string | null) {
  if (!value) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

function parseSortOrder(
  value: string | null,
  fallback: 'asc' | 'desc',
): 'asc' | 'desc' {
  return value === 'asc' || value === 'desc' ? value : fallback;
}

export function useTableSearchParams({
  defaultSortBy,
  defaultSortOrder = 'desc',
}: UseTableSearchParamsOptions) {
  const [searchParams, setSearchParams] = useSearchParams();

  const state = useMemo<ControlledServerTableState>(() => {
    const page = parsePositiveInt(searchParams.get('page')) ?? 1;
    const search = searchParams.get('query') ?? '';
    const sortBy = searchParams.get('sort_by') ?? defaultSortBy;
    const sortOrder = parseSortOrder(
      searchParams.get('sort_order'),
      defaultSortOrder,
    );

    return {
      page,
      search,
      sortBy,
      sortOrder,
    };
  }, [defaultSortBy, defaultSortOrder, searchParams]);

  const updateParams = useCallback(
    (
      updates: {
        page?: number;
        search?: string;
        sortBy?: string | null;
        sortOrder?: 'asc' | 'desc';
      },
      options?: { replace?: boolean },
    ) => {
      const next = new URLSearchParams(searchParams);
      const nextPage = updates.page ?? state.page;
      const nextSearch = updates.search ?? state.search;
      const nextSortBy =
        updates.sortBy === undefined ? state.sortBy : updates.sortBy;
      const nextSortOrder = updates.sortOrder ?? state.sortOrder;

      if (nextPage <= 1) {
        next.delete('page');
      } else {
        next.set('page', String(nextPage));
      }

      if (nextSearch) {
        next.set('query', nextSearch);
      } else {
        next.delete('query');
      }

      if (nextSortBy && nextSortBy !== defaultSortBy) {
        next.set('sort_by', nextSortBy);
      } else {
        next.delete('sort_by');
      }

      const shouldPersistSortOrder =
        !!nextSortBy && nextSortOrder !== defaultSortOrder;

      if (shouldPersistSortOrder) {
        next.set('sort_order', nextSortOrder);
      } else {
        next.delete('sort_order');
      }

      setSearchParams(next, { replace: options?.replace ?? false });
    },
    [defaultSortBy, defaultSortOrder, searchParams, setSearchParams, state],
  );

  const setPage = useCallback(
    (page: number) => {
      updateParams({ page });
    },
    [updateParams],
  );

  const setSearch = useCallback(
    (search: string) => {
      updateParams({ search, page: 1 }, { replace: true });
    },
    [updateParams],
  );

  const setSort = useCallback(
    (sortBy: string | undefined, sortOrder: 'asc' | 'desc') => {
      updateParams(
        {
          sortBy: sortBy ?? null,
          sortOrder,
          page: 1,
        },
        { replace: true },
      );
    },
    [updateParams],
  );

  return {
    state,
    setPage,
    setSearch,
    setSort,
  };
}
