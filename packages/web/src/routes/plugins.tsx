import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import { usePluginRegistry } from '@broccoli/web-sdk/plugin';
import { Card, CardContent, CardHeader, Skeleton } from '@broccoli/web-sdk/ui';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { AlertCircle, Puzzle } from 'lucide-react';
import { useCallback, useState } from 'react';
import { toast } from 'sonner';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { PluginCard } from '@/features/plugin/components/PluginCard';

export default function PluginsPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const [togglingIds, setTogglingIds] = useState<Set<string>>(() => new Set());
  const { unloadPlugin } = usePluginRegistry();

  const {
    data: plugins,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['admin-plugins'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/admin/plugins');
      if (error) throw error;
      return data;
    },
  });

  const handleToggle = useCallback(
    async (plugin: PluginDetail, enable: boolean) => {
      setTogglingIds((prev) => new Set(prev).add(plugin.id));
      try {
        const endpoint = enable
          ? '/admin/plugins/{id}/enable'
          : '/admin/plugins/{id}/disable';

        const { error } = await apiClient.POST(endpoint, {
          params: { path: { id: plugin.id } },
        });

        if (error) {
          const msg =
            error && typeof error === 'object' && 'message' in error
              ? (error as { message?: string }).message
              : undefined;
          toast.error(
            msg || `Failed to ${enable ? 'enable' : 'disable'} plugin`,
          );
        } else {
          queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
          queryClient.invalidateQueries({ queryKey: ['i18n'] });

          // Unnecessary to load the plugin immediately, as it will be lazily
          // loaded when the user navigates to a page that uses it.
          if (!enable) {
            unloadPlugin(plugin.id);
          }
        }
      } finally {
        setTogglingIds((prev) => {
          const next = new Set(prev);
          next.delete(plugin.id);
          return next;
        });
      }
    },
    [apiClient, queryClient, unloadPlugin],
  );

  if (!user || !user.permissions.includes('plugin:manage')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="plugins"
      icon={<Puzzle className="h-6 w-6 text-primary" />}
      title={t('plugins.title')}
      subtitle={t('plugins.subtitle')}
    >
      {isLoading && (
        <div className="grid gap-4 md:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <Card key={`skeleton-${String(i)}`}>
              <CardHeader>
                <Skeleton className="h-5 w-32" />
                <Skeleton className="h-4 w-48 mt-1" />
              </CardHeader>
              <CardContent>
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4 mt-2" />
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6 text-center">
            <AlertCircle className="mx-auto h-10 w-10 text-destructive mb-2" />
            <p className="text-destructive">{t('plugins.loadError')}</p>
          </CardContent>
        </Card>
      )}

      {plugins && plugins.length === 0 && (
        <Card>
          <CardContent className="pt-6 text-center">
            <Puzzle className="mx-auto h-10 w-10 text-muted-foreground mb-2" />
            <p className="text-muted-foreground">{t('plugins.empty')}</p>
          </CardContent>
        </Card>
      )}

      {plugins && plugins.length > 0 && (
        <div className="grid gap-4 md:grid-cols-2">
          {plugins.map((plugin) => (
            <PluginCard
              key={plugin.id}
              plugin={plugin}
              toggling={togglingIds.has(plugin.id)}
              onToggle={handleToggle}
            />
          ))}
        </div>
      )}
    </PageLayout>
  );
}
