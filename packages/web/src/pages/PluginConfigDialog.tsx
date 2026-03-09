import type { PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useEffect, useMemo, useState } from 'react';

import { ConfigForm } from '@/components/config';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface PluginConfigDialogProps {
  plugin: PluginDetailResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function PluginConfigDialog({
  plugin,
  open,
  onOpenChange,
}: PluginConfigDialogProps) {
  const schemas = plugin.config_schemas;
  const [activeTab, setActiveTab] = useState(schemas[0]?.namespace ?? '');
  const { t } = useTranslation();
  const apiClient = useApiClient();

  useEffect(() => {
    if (open && schemas.length > 0) {
      setActiveTab(schemas[0].namespace);
    }
  }, [open, schemas]);

  const callbacksByNamespace = useMemo(
    () =>
      Object.fromEntries(
        schemas.map((s) => [
          s.namespace,
          {
            getConfig: async () => {
              const { data, error } = await apiClient.GET(
                '/admin/plugins/{id}/config/{namespace}',
                {
                  params: { path: { id: plugin.id, namespace: s.namespace } },
                },
              );
              if (error) throw error;
              return (data?.config ?? {}) as Record<string, unknown>;
            },
            putConfig: async (config: Record<string, unknown>) => {
              const { error } = await apiClient.PUT(
                '/admin/plugins/{id}/config/{namespace}',
                {
                  params: { path: { id: plugin.id, namespace: s.namespace } },
                  body: { config },
                },
              );
              return { error };
            },
            deleteConfig: async () => {
              const { error } = await apiClient.DELETE(
                '/admin/plugins/{id}/config/{namespace}',
                {
                  params: { path: { id: plugin.id, namespace: s.namespace } },
                },
              );
              return { error };
            },
          },
        ]),
      ),
    [plugin.id, schemas, apiClient],
  );

  if (schemas.length === 0) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {t('plugins.config.title', { name: plugin.name })}
          </DialogTitle>
          <DialogDescription>
            {t('plugins.config.description')}
          </DialogDescription>
        </DialogHeader>

        {schemas.length === 1 ? (
          <ConfigForm
            schema={schemas[0]}
            open={open}
            pluginId={plugin.id}
            {...callbacksByNamespace[schemas[0].namespace]}
            invalidateQueryKeys={[['admin-plugins']]}
          />
        ) : (
          <Tabs value={activeTab} onValueChange={setActiveTab}>
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
                  open={open && activeTab === s.namespace}
                  pluginId={plugin.id}
                  {...callbacksByNamespace[s.namespace]}
                  invalidateQueryKeys={[['admin-plugins']]}
                />
              </TabsContent>
            ))}
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}
