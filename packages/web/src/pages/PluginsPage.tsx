import type { PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { usePluginRegistry } from '@broccoli/sdk/plugin';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  AlertCircle,
  Cpu,
  Globe,
  Loader2,
  Puzzle,
  Server,
  Shield,
} from 'lucide-react';
import { useCallback, useState } from 'react';

import { Badge } from '@/components/ui/badge';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Switch } from '@/components/ui/switch';
import { useAuth } from '@/contexts/auth-context';

export function PluginsPage() {
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
    async (plugin: PluginDetailResponse, enable: boolean) => {
      setTogglingIds((prev) => new Set(prev).add(plugin.id));
      try {
        const endpoint = enable
          ? '/admin/plugins/{id}/enable'
          : '/admin/plugins/{id}/disable';

        const { error } = await apiClient.POST(endpoint, {
          params: { path: { id: plugin.id } },
        });

        if (!error) {
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
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="max-w-md">
          <CardContent className="pt-6 text-center">
            <Shield className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
            <p className="text-destructive text-lg font-medium">
              {t('admin.unauthorized')}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="mx-auto w-[75%] p-6 space-y-6">
      <div className="flex items-center gap-3">
        <Puzzle className="h-6 w-6 text-primary" />
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            {t('plugins.title')}
          </h1>
          <p className="text-muted-foreground">{t('plugins.subtitle')}</p>
        </div>
      </div>

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
    </div>
  );
}

function PluginCard({
  plugin,
  toggling,
  onToggle,
}: {
  plugin: PluginDetailResponse;
  toggling: boolean;
  onToggle: (plugin: PluginDetailResponse, enable: boolean) => void;
}) {
  const { t } = useTranslation();
  const isEnabled = plugin.status === 'Loaded';

  return (
    <Card className="flex flex-col">
      <CardHeader>
        <CardTitle className="text-base flex items-center gap-2">
          <span className="truncate">{plugin.name}</span>
        </CardTitle>
        <CardDescription className="text-xs font-mono">
          {plugin.id}
          {plugin.version && (
            <span className="ml-2 text-muted-foreground">
              v{plugin.version}
            </span>
          )}
        </CardDescription>
      </CardHeader>

      <CardContent className="flex-1">
        {plugin.description && (
          <p className="text-sm text-muted-foreground mb-3">
            {plugin.description}
          </p>
        )}
        <div className="flex items-end justify-between gap-4">
          <div className="flex flex-wrap gap-2">
            {plugin.has_server && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Server className="h-3 w-3" />
                {t('plugins.component.server')}
              </Badge>
            )}
            {plugin.has_web && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Globe className="h-3 w-3" />
                {t('plugins.component.web')}
              </Badge>
            )}
            {plugin.has_worker && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Cpu className="h-3 w-3" />
                {t('plugins.component.worker')}
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {toggling && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
            <Switch
              checked={isEnabled}
              disabled={toggling || plugin.status === 'Failed'}
              onCheckedChange={(checked) => onToggle(plugin, checked)}
              aria-label={
                isEnabled ? t('plugins.disable') : t('plugins.enable')
              }
            />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
