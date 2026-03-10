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
  RefreshCw,
  Server,
  Settings,
} from 'lucide-react';
import { useCallback, useState } from 'react';

import { ResourceConfigDialog } from '@/components/config';
import { PageLayout } from '@/components/PageLayout';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Switch } from '@/components/ui/switch';
import { Unauthorized } from '@/components/Unauthorized';
import { useAuth } from '@/contexts/auth-context';

export function PluginsPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const [togglingIds, setTogglingIds] = useState<Set<string>>(() => new Set());
  const [reloadingIds, setReloadingIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [isReloadingAll, setIsReloadingAll] = useState(false);
  const { unloadPlugin, reloadPlugin, reloadAllPlugins } = usePluginRegistry();

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

  const handleReload = useCallback(
    async (plugin: PluginDetailResponse) => {
      setReloadingIds((prev) => new Set(prev).add(plugin.id));
      try {
        const { error } = await apiClient.POST('/admin/plugins/{id}/reload', {
          params: { path: { id: plugin.id } },
        });

        if (!error) {
          await reloadPlugin(plugin.id);
          queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
          queryClient.invalidateQueries({ queryKey: ['i18n'] });
        }
      } finally {
        setReloadingIds((prev) => {
          const next = new Set(prev);
          next.delete(plugin.id);
          return next;
        });
      }
    },
    [apiClient, queryClient, reloadPlugin],
  );

  const handleReloadAll = useCallback(async () => {
    setIsReloadingAll(true);
    try {
      const { error } = await apiClient.POST('/admin/plugins/reload-all', {});

      if (!error) {
        await reloadAllPlugins();
        queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
        queryClient.invalidateQueries({ queryKey: ['i18n'] });
      }
    } finally {
      setIsReloadingAll(false);
    }
  }, [apiClient, queryClient, reloadAllPlugins]);

  if (!user || !user.permissions.includes('plugin:manage')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="plugins"
      icon={<Puzzle className="h-6 w-6 text-primary" />}
      title={t('plugins.title')}
      subtitle={t('plugins.subtitle')}
      actions={
        <Button
          variant="outline"
          size="sm"
          onClick={handleReloadAll}
          disabled={isReloadingAll || isLoading}
        >
          {isReloadingAll ? (
            <Loader2 className="h-4 w-4 animate-spin mr-2" />
          ) : (
            <RefreshCw className="h-4 w-4 mr-2" />
          )}
          {t('plugins.reloadAll')}
        </Button>
      }
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
              reloading={reloadingIds.has(plugin.id)}
              onToggle={handleToggle}
              onReload={handleReload}
            />
          ))}
        </div>
      )}
    </PageLayout>
  );
}

/** Wrapper that manages the config dialog state for a single plugin. */
function PluginConfigButton({ plugin }: { plugin: PluginDetailResponse }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const hasPluginSchemas = plugin.config_schemas?.some((s) =>
    s.scopes.includes('plugin'),
  );
  if (!hasPluginSchemas) return null;

  return (
    <>
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8"
        onClick={() => setOpen(true)}
        aria-label={t('plugins.configure')}
        title={t('plugins.configure')}
      >
        <Settings className="h-4 w-4" />
      </Button>
      <ResourceConfigDialog
        scope={{ scope: 'plugin', pluginId: plugin.id }}
        resourceLabel={plugin.name}
        open={open}
        onOpenChange={setOpen}
      />
    </>
  );
}

function PluginCard({
  plugin,
  toggling,
  reloading,
  onToggle,
  onReload,
}: {
  plugin: PluginDetailResponse;
  toggling: boolean;
  reloading: boolean;
  onToggle: (plugin: PluginDetailResponse, enable: boolean) => void;
  onReload: (plugin: PluginDetailResponse) => void;
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
          <div className="flex items-center gap-1 shrink-0">
            <PluginConfigButton plugin={plugin} />
            {isEnabled && (
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => onReload(plugin)}
                disabled={reloading || toggling}
                aria-label={t('plugins.reload')}
                title={t('plugins.reload')}
              >
                {reloading ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="h-4 w-4" />
                )}
              </Button>
            )}
            {toggling && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
            <Switch
              checked={isEnabled}
              disabled={toggling || reloading || plugin.status === 'Failed'}
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
