import { useQuery } from '@tanstack/react-query';

import { useApiClient } from '@/api';

export function useRegistries() {
  const apiClient = useApiClient();

  return useQuery({
    queryKey: ['plugin-registries'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/plugins/registries');
      if (error) throw error;
      return data;
    },
    staleTime: 60_000,
  });
}
