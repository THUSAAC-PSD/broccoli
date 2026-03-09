import type { PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useQuery } from '@tanstack/react-query';

/**
 * Returns whether any loaded plugin has config schemas for the given scope.
 * Uses the same `['admin-plugins']` query key as ResourceConfigDialog,
 * so the data is shared via react-query's cache.
 */
export function useHasConfigSchemas(scope: string): boolean {
  const apiClient = useApiClient();

  const { data: plugins = [] } = useQuery({
    queryKey: ['admin-plugins'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/admin/plugins');
      if (error) throw error;
      return data;
    },
  });

  return plugins.some((p: PluginDetailResponse) =>
    p.config_schemas.some((s) => s.scopes.includes(scope)),
  );
}
