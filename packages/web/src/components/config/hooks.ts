import { useApiClient } from '@broccoli/web-sdk/api';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import { useQuery } from '@tanstack/react-query';

type PluginDetailResponse = PluginDetail;

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
