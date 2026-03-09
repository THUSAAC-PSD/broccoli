import type { ConfigSchemaResponse, PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

import { ConfigForm } from './ConfigForm';
import type { ConfigScope } from './types';

export interface ResourceConfigDialogProps {
  scope: ConfigScope;
  resourceLabel: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function scopeString(scope: ConfigScope): string {
  return scope.scope;
}

function buildConfigCallbacks(
  apiClient: ReturnType<typeof useApiClient>,
  scope: ConfigScope,
  namespace: string,
) {
  switch (scope.scope) {
    case 'plugin':
      return {
        getConfig: async () => {
          const { data, error } = await apiClient.GET(
            '/admin/plugins/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.pluginId, namespace },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (config: Record<string, unknown>) => {
          const { error } = await apiClient.PUT(
            '/admin/plugins/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.pluginId, namespace },
              },
              body: { config },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/admin/plugins/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.pluginId, namespace },
              },
            },
          );
          return { error };
        },
      };
    case 'contest':
      return {
        getConfig: async () => {
          const { data, error } = await apiClient.GET(
            '/contests/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.contestId, namespace },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (config: Record<string, unknown>) => {
          const { error } = await apiClient.PUT(
            '/contests/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.contestId, namespace },
              },
              body: { config },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/contests/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.contestId, namespace },
              },
            },
          );
          return { error };
        },
      };
    case 'problem':
      return {
        getConfig: async () => {
          const { data, error } = await apiClient.GET(
            '/problems/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.problemId, namespace },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (config: Record<string, unknown>) => {
          const { error } = await apiClient.PUT(
            '/problems/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.problemId, namespace },
              },
              body: { config },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/problems/{id}/config/{namespace}',
            {
              params: {
                path: { id: scope.problemId, namespace },
              },
            },
          );
          return { error };
        },
      };
    case 'contest_problem':
      return {
        getConfig: async () => {
          const { data, error } = await apiClient.GET(
            '/contests/{id}/problems/{problem_id}/config/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  namespace,
                },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (config: Record<string, unknown>) => {
          const { error } = await apiClient.PUT(
            '/contests/{id}/problems/{problem_id}/config/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  namespace,
                },
              },
              body: { config },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/contests/{id}/problems/{problem_id}/config/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  namespace,
                },
              },
            },
          );
          return { error };
        },
      };
  }
}

export function ResourceConfigDialog({
  scope,
  resourceLabel,
  open,
  onOpenChange,
}: ResourceConfigDialogProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const scopeStr = scopeString(scope);

  const { data: plugins = [] } = useQuery({
    queryKey: ['admin-plugins'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/admin/plugins');
      if (error) throw error;
      return data;
    },
  });

  const pluginsWithSchemas = useMemo(
    () =>
      plugins
        .filter((plugin: PluginDetailResponse) =>
          scope.scope === 'plugin' ? plugin.id === scope.pluginId : true,
        )
        .map((plugin: PluginDetailResponse) => ({
          plugin,
          schemas: plugin.config_schemas.filter((s: ConfigSchemaResponse) =>
            s.scopes.includes(scopeStr),
          ),
        }))
        .filter(
          (entry: {
            plugin: PluginDetailResponse;
            schemas: ConfigSchemaResponse[];
          }) => entry.schemas.length > 0,
        ),
    [plugins, scopeStr, scope],
  );

  const [activePlugin, setActivePlugin] = useState('');
  const [activeNamespace, setActiveNamespace] = useState('');

  useEffect(() => {
    if (open && pluginsWithSchemas.length > 0) {
      setActivePlugin(pluginsWithSchemas[0].plugin.id);
      setActiveNamespace(pluginsWithSchemas[0].schemas[0]?.namespace ?? '');
    }
  }, [open, pluginsWithSchemas]);

  const invalidateKeys = [['admin-plugins']];
  if (scope.scope === 'contest' || scope.scope === 'contest_problem')
    invalidateKeys.push(['admin-contests']);
  if (scope.scope === 'problem' || scope.scope === 'contest_problem')
    invalidateKeys.push(['admin-problems']);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {t('config.title', { name: resourceLabel })}
          </DialogTitle>
          <DialogDescription>{t('config.description')}</DialogDescription>
        </DialogHeader>

        {pluginsWithSchemas.length === 0 ? (
          <div className="py-12 text-center text-muted-foreground text-sm">
            {t('config.noSchemas')}
          </div>
        ) : pluginsWithSchemas.length === 1 ? (
          <SinglePluginContent
            pluginId={pluginsWithSchemas[0].plugin.id}
            schemas={pluginsWithSchemas[0].schemas}
            apiClient={apiClient}
            scope={scope}
            open={open}
            invalidateKeys={invalidateKeys}
          />
        ) : (
          <Tabs
            value={activePlugin}
            onValueChange={(v) => {
              setActivePlugin(v);
              const entry = pluginsWithSchemas.find(
                (e: { plugin: PluginDetailResponse }) => e.plugin.id === v,
              );
              if (entry) setActiveNamespace(entry.schemas[0]?.namespace ?? '');
            }}
          >
            <TabsList>
              {pluginsWithSchemas.map(
                (entry: {
                  plugin: PluginDetailResponse;
                  schemas: ConfigSchemaResponse[];
                }) => (
                  <TabsTrigger key={entry.plugin.id} value={entry.plugin.id}>
                    {entry.plugin.name}
                  </TabsTrigger>
                ),
              )}
            </TabsList>
            {pluginsWithSchemas.map(
              (entry: {
                plugin: PluginDetailResponse;
                schemas: ConfigSchemaResponse[];
              }) => (
                <TabsContent key={entry.plugin.id} value={entry.plugin.id}>
                  <SinglePluginContent
                    pluginId={entry.plugin.id}
                    schemas={entry.schemas}
                    apiClient={apiClient}
                    scope={scope}
                    open={open && activePlugin === entry.plugin.id}
                    invalidateKeys={invalidateKeys}
                    activeNamespace={activeNamespace}
                    onNamespaceChange={setActiveNamespace}
                  />
                </TabsContent>
              ),
            )}
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}

function SinglePluginContent({
  pluginId,
  schemas,
  apiClient,
  scope,
  open,
  invalidateKeys,
  activeNamespace: controlledNamespace,
  onNamespaceChange,
}: {
  pluginId: string;
  schemas: ConfigSchemaResponse[];
  apiClient: ReturnType<typeof useApiClient>;
  scope: ConfigScope;
  open: boolean;
  invalidateKeys: string[][];
  activeNamespace?: string;
  onNamespaceChange?: (ns: string) => void;
}) {
  const [localNamespace, setLocalNamespace] = useState(
    schemas[0]?.namespace ?? '',
  );
  const activeNs = controlledNamespace ?? localNamespace;
  const setActiveNs = onNamespaceChange ?? setLocalNamespace;

  useEffect(() => {
    if (open && schemas.length > 0 && !controlledNamespace) {
      setLocalNamespace(schemas[0].namespace);
    }
  }, [open, schemas, controlledNamespace]);

  const callbacksByNamespace = useMemo(
    () =>
      Object.fromEntries(
        schemas.map((s) => [
          s.namespace,
          buildConfigCallbacks(apiClient, scope, s.namespace),
        ]),
      ),
    [apiClient, scope, schemas],
  );

  if (schemas.length === 1) {
    return (
      <ConfigForm
        schema={schemas[0]}
        open={open}
        pluginId={pluginId}
        {...callbacksByNamespace[schemas[0].namespace]}
        invalidateQueryKeys={invalidateKeys}
      />
    );
  }

  return (
    <Tabs value={activeNs} onValueChange={setActiveNs}>
      <TabsList>
        {schemas.map((s) => (
          <TabsTrigger key={s.namespace} value={s.namespace}>
            {s.namespace}
          </TabsTrigger>
        ))}
      </TabsList>
      {schemas.map((s) => (
        <TabsContent key={s.namespace} value={s.namespace}>
          <ConfigForm
            schema={s}
            open={open && activeNs === s.namespace}
            pluginId={pluginId}
            {...callbacksByNamespace[s.namespace]}
            invalidateQueryKeys={invalidateKeys}
          />
        </TabsContent>
      ))}
    </Tabs>
  );
}
