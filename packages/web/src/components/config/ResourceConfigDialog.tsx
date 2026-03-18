import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import {
  Label,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
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

/** A config entry returned by resource-scoped config list endpoints. */
interface ConfigEntry {
  plugin_id: string;
  namespace: string;
  config: unknown;
  enabled: boolean;
  position: number;
  updated_at: string | null;
  json_schema?: Record<string, unknown>;
  description?: string | null;
}

export interface ResourceConfigDialogProps {
  scope: ConfigScope;
  resourceLabel: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
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

/** Fetch the config list for a resource-scoped config (problem/contest/contest_problem).
 *  Returns config entries with embedded json_schema from the self-describing endpoint. */
function useResourceConfigList(
  apiClient: ReturnType<typeof useApiClient>,
  scope: ConfigScope,
  open: boolean,
) {
  return useQuery<ConfigEntry[]>({
    queryKey: configListQueryKey(scope),
    queryFn: async () => {
      let result: { data?: unknown; error?: unknown };

      switch (scope.scope) {
        case 'problem':
          result = await apiClient.GET('/problems/{id}/config', {
            params: { path: { id: scope.problemId } },
          });
          break;
        case 'contest':
          result = await apiClient.GET('/contests/{id}/config', {
            params: { path: { id: scope.contestId } },
          });
          break;
        case 'contest_problem':
          result = await apiClient.GET(
            '/contests/{id}/problems/{problem_id}/config',
            {
              params: {
                path: {
                  id: scope.contestId,
                  problem_id: scope.problemId,
                },
              },
            },
          );
          break;
        default:
          return [];
      }

      if (result.error) throw result.error;
      return (result.data ?? []) as ConfigEntry[]; // All three endpoints return PluginConfigResponse[]
    },
    enabled: open && scope.scope !== 'plugin',
  });
}

function configListQueryKey(scope: ConfigScope): string[] {
  switch (scope.scope) {
    case 'problem':
      return ['config-list', 'problem', String(scope.problemId)];
    case 'contest':
      return ['config-list', 'contest', String(scope.contestId)];
    case 'contest_problem':
      return [
        'config-list',
        'contest_problem',
        String(scope.contestId),
        String(scope.problemId),
      ];
    case 'plugin':
      return ['config-list', 'plugin', scope.pluginId];
  }
}

/** Convert config list entries into the pluginsWithSchemas structure the dialog expects. */
function configEntriesToPluginSchemas(
  entries: ConfigEntry[],
): { pluginId: string; schemas: ConfigSchemaResponse[] }[] {
  const byPlugin = new Map<
    string,
    { pluginId: string; schemas: ConfigSchemaResponse[] }
  >();

  for (const entry of entries) {
    if (!entry.json_schema) continue;

    if (!byPlugin.has(entry.plugin_id)) {
      byPlugin.set(entry.plugin_id, {
        pluginId: entry.plugin_id,
        schemas: [],
      });
    }

    byPlugin.get(entry.plugin_id)!.schemas.push({
      namespace: entry.namespace,
      description: entry.description ?? undefined,
      scopes: [], // Not needed since we already know the scope matches
      json_schema: entry.json_schema as Record<string, unknown>,
    });
  }

  return Array.from(byPlugin.values());
}

/** For plugin scope, use GET /admin/plugins (admin-only). */
function usePluginScopeSchemas(
  apiClient: ReturnType<typeof useApiClient>,
  scope: ConfigScope,
  open: boolean,
) {
  const { data: plugins = [] } = useQuery({
    queryKey: ['admin-plugins'],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/admin/plugins');
      if (error) throw error;
      return data;
    },
    enabled: open && scope.scope === 'plugin',
  });

  return useMemo(() => {
    if (scope.scope !== 'plugin') return [];
    return plugins
      .filter((p: PluginDetailResponse) => p.id === scope.pluginId)
      .map((p: PluginDetailResponse) => ({
        pluginId: p.id,
        schemas: p.config_schemas.filter((s: ConfigSchemaResponse) =>
          s.scopes.includes('plugin'),
        ),
      }))
      .filter((e: { schemas: ConfigSchemaResponse[] }) => e.schemas.length > 0);
  }, [plugins, scope]);
}

export function ResourceConfigDialog({
  scope,
  resourceLabel,
  open,
  onOpenChange,
}: ResourceConfigDialogProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();

  // For plugin scope, use admin endpoint
  const pluginSchemas = usePluginScopeSchemas(apiClient, scope, open);

  // For resource scopes, use self-describing config list endpoint
  const { data: configEntries = [] } = useResourceConfigList(
    apiClient,
    scope,
    open,
  );

  const pluginsWithSchemas = useMemo(() => {
    if (scope.scope === 'plugin') return pluginSchemas;
    return configEntriesToPluginSchemas(configEntries);
  }, [scope, pluginSchemas, configEntries]);

  const [activePlugin, setActivePlugin] = useState('');
  const [activeNamespace, setActiveNamespace] = useState('');

  useEffect(() => {
    if (open && pluginsWithSchemas.length > 0) {
      setActivePlugin(pluginsWithSchemas[0].pluginId);
      setActiveNamespace(pluginsWithSchemas[0].schemas[0]?.namespace ?? '');
    }
  }, [open, pluginsWithSchemas]);

  const invalidateKeys = [configListQueryKey(scope)];
  if (scope.scope === 'plugin') invalidateKeys.push(['admin-plugins']);
  if (scope.scope === 'contest' || scope.scope === 'contest_problem')
    invalidateKeys.push(['admin-contests']);
  if (scope.scope === 'problem' || scope.scope === 'contest_problem')
    invalidateKeys.push(['admin-problems']);

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="right"
        size="3xl"
        className="flex flex-col overflow-hidden p-0"
      >
        <SheetHeader className="shrink-0 border-b px-6 py-4">
          <SheetTitle>{t('config.title', { name: resourceLabel })}</SheetTitle>
          <SheetDescription>{t('config.description')}</SheetDescription>
        </SheetHeader>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {pluginsWithSchemas.length === 0 ? (
            <div className="py-12 text-center text-muted-foreground text-sm">
              {t('config.noSchemas')}
            </div>
          ) : pluginsWithSchemas.length === 1 ? (
            <SinglePluginContent
              pluginId={pluginsWithSchemas[0].pluginId}
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
                const entry = pluginsWithSchemas.find((e) => e.pluginId === v);
                if (entry)
                  setActiveNamespace(entry.schemas[0]?.namespace ?? '');
              }}
            >
              <TabsList>
                {pluginsWithSchemas.map((entry) => (
                  <TabsTrigger key={entry.pluginId} value={entry.pluginId}>
                    {entry.pluginId}
                  </TabsTrigger>
                ))}
              </TabsList>
              {pluginsWithSchemas.map((entry) => (
                <TabsContent key={entry.pluginId} value={entry.pluginId}>
                  <SinglePluginContent
                    pluginId={entry.pluginId}
                    schemas={entry.schemas}
                    apiClient={apiClient}
                    scope={scope}
                    open={open && activePlugin === entry.pluginId}
                    invalidateKeys={invalidateKeys}
                    activeNamespace={activeNamespace}
                    onNamespaceChange={setActiveNamespace}
                  />
                </TabsContent>
              ))}
            </Tabs>
          )}
        </div>
      </SheetContent>
    </Sheet>
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
