import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Label,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';

import { ConfigForm } from './ConfigForm';
import type { ConfigScope } from './types';

type ConfigSchemaResponse = PluginDetail['config_schemas'][number];
type PluginDetailResponse = PluginDetail;

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
  pluginId: string,
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
            '/contests/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.contestId, plugin_id: pluginId, namespace },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (
          config: Record<string, unknown>,
          enabled?: boolean,
        ) => {
          const { error } = await apiClient.PUT(
            '/contests/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.contestId, plugin_id: pluginId, namespace },
              },
              body: { config, ...(enabled !== undefined ? { enabled } : {}) },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/contests/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.contestId, plugin_id: pluginId, namespace },
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
            '/problems/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.problemId, plugin_id: pluginId, namespace },
              },
            },
          );
          if (error) throw error;
          return (data?.config ?? {}) as Record<string, unknown>;
        },
        putConfig: async (
          config: Record<string, unknown>,
          enabled?: boolean,
        ) => {
          const { error } = await apiClient.PUT(
            '/problems/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.problemId, plugin_id: pluginId, namespace },
              },
              body: { config, ...(enabled !== undefined ? { enabled } : {}) },
            },
          );
          return { error };
        },
        deleteConfig: async () => {
          const { error } = await apiClient.DELETE(
            '/problems/{id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: { id: scope.problemId, plugin_id: pluginId, namespace },
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
            '/contests/{id}/problems/{problem_id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  plugin_id: pluginId,
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
            '/contests/{id}/problems/{problem_id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  plugin_id: pluginId,
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
            '/contests/{id}/problems/{problem_id}/config/{plugin_id}/{namespace}',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                  plugin_id: pluginId,
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
  const { t } = useTranslation();
  const [localNamespace, setLocalNamespace] = useState(
    schemas[0]?.namespace ?? '',
  );
  const activeNs = controlledNamespace ?? localNamespace;
  const setActiveNs = onNamespaceChange ?? setLocalNamespace;

  const showEnabledToggle =
    scope.scope === 'contest' || scope.scope === 'problem';
  const [enabled, setEnabled] = useState(true);
  const enabledRef = useRef(true);
  enabledRef.current = enabled;

  useEffect(() => {
    if (!open || !showEnabledToggle || schemas.length === 0) return;
    const firstNs = schemas[0].namespace;

    // Load enabled from the first namespace's config row
    let req: Promise<{ data?: { enabled: boolean }; error?: unknown }>;
    if (scope.scope === 'contest') {
      req = apiClient.GET('/contests/{id}/config/{plugin_id}/{namespace}', {
        params: {
          path: {
            id: scope.contestId,
            plugin_id: pluginId,
            namespace: firstNs,
          },
        },
      });
    } else if (scope.scope === 'problem') {
      req = apiClient.GET('/problems/{id}/config/{plugin_id}/{namespace}', {
        params: {
          path: {
            id: scope.problemId,
            plugin_id: pluginId,
            namespace: firstNs,
          },
        },
      });
    } else {
      return;
    }

    req
      .then(({ data }) => {
        setEnabled(data?.enabled ?? true);
      })
      .catch(() => setEnabled(true));
  }, [open, showEnabledToggle, schemas, apiClient, scope, pluginId]);

  useEffect(() => {
    if (open && schemas.length > 0 && !controlledNamespace) {
      setLocalNamespace(schemas[0].namespace);
    }
  }, [open, schemas, controlledNamespace]);

  const rawCallbacks = useMemo(
    () =>
      Object.fromEntries(
        schemas.map((s) => [
          s.namespace,
          buildConfigCallbacks(apiClient, scope, pluginId, s.namespace),
        ]),
      ),
    [apiClient, scope, pluginId, schemas],
  );

  const callbacksByNamespace = useMemo(() => {
    if (!showEnabledToggle) return rawCallbacks;
    return Object.fromEntries(
      Object.entries(rawCallbacks).map(([ns, cbs]) => [
        ns,
        {
          ...cbs,
          putConfig: (config: Record<string, unknown>) =>
            (
              cbs.putConfig as (
                c: Record<string, unknown>,
                e?: boolean,
              ) => Promise<{ error?: unknown }>
            )(config, enabledRef.current),
        },
      ]),
    );
  }, [rawCallbacks, showEnabledToggle]);

  const enabledToggle = showEnabledToggle ? (
    <div className="flex items-center justify-between rounded-lg border p-3">
      <Label
        htmlFor={`plugin-enabled-${pluginId}`}
        className="text-sm font-medium"
      >
        {t('plugins.config.enabled')}
      </Label>
      <Switch
        id={`plugin-enabled-${pluginId}`}
        checked={enabled}
        onCheckedChange={setEnabled}
      />
    </div>
  ) : null;

  if (schemas.length === 1) {
    return (
      <div className="space-y-4">
        {enabledToggle}
        <ConfigForm
          schema={schemas[0]}
          open={open}
          pluginId={pluginId}
          scope={scope}
          {...callbacksByNamespace[schemas[0].namespace]}
          invalidateQueryKeys={invalidateKeys}
        />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {enabledToggle}
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
              scope={scope}
              {...callbacksByNamespace[s.namespace]}
              invalidateQueryKeys={invalidateKeys}
            />
          </TabsContent>
        ))}
      </Tabs>
    </div>
  );
}
