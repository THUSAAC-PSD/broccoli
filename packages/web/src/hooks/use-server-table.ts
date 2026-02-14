import { type ApiClient, useApiClient } from '@broccoli/sdk/api';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface ServerTableParams {
  page: number;
  per_page: number;
  search?: string;
  sort_by?: string;
  sort_order?: 'asc' | 'desc';
}

export interface PaginationMeta {
  page: number;
  per_page: number;
  total: number;
  total_pages: number;
}

export interface ServerTableResponse<T> {
  data: T[];
  pagination: PaginationMeta;
}

export interface UseServerTableOptions<T> {
  queryKey: string[];
  fetchFn: (
    apiClient: ApiClient,
    params: ServerTableParams,
  ) => Promise<ServerTableResponse<T>>;
  defaultPerPage?: number;
  defaultSortBy?: string;
  defaultSortOrder?: 'asc' | 'desc';
  debounceMs?: number;
}

export function useServerTable<T>({
  queryKey,
  fetchFn,
  defaultPerPage = 20,
  defaultSortBy,
  defaultSortOrder = 'desc',
  debounceMs = 300,
}: UseServerTableOptions<T>) {
  const [page, setPage] = useState(1);
  const [perPage, setPerPage] = useState(defaultPerPage);
  const [search, setSearch] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');
  const [sortBy, setSortBy] = useState(defaultSortBy);
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>(defaultSortOrder);
  const debounceTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const apiClient = useApiClient();

  useEffect(() => {
    debounceTimer.current = setTimeout(() => {
      setDebouncedSearch(search);
      setPage(1);
    }, debounceMs);
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    };
  }, [search, debounceMs]);

  const params: ServerTableParams = {
    page,
    per_page: perPage,
    ...(debouncedSearch && { search: debouncedSearch }),
    ...(sortBy && { sort_by: sortBy }),
    ...(sortBy && { sort_order: sortOrder }),
  };

  const { data, isLoading, isFetching } = useQuery({
    queryKey: [...queryKey, params],
    queryFn: () => fetchFn(apiClient, params),
    placeholderData: keepPreviousData,
  });

  const toggleSort = useCallback(
    (column: string) => {
      if (sortBy === column) {
        setSortOrder((prev) => (prev === 'asc' ? 'desc' : 'asc'));
      } else {
        setSortBy(column);
        setSortOrder('asc');
      }
      setPage(1);
    },
    [sortBy],
  );

  const pagination = data?.pagination ?? {
    page: 1,
    per_page: perPage,
    total: 0,
    total_pages: 0,
  };

  return {
    data: data?.data ?? [],
    pagination,
    isLoading,
    isFetching,
    page,
    perPage,
    search,
    sortBy,
    sortOrder,
    setPage,
    setPerPage: (v: number) => {
      setPerPage(v);
      setPage(1);
    },
    setSearch,
    toggleSort,
  };
}
