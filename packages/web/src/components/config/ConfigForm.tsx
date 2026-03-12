import type { ConfigSchemaResponse } from '@broccoli/sdk';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { useQueryClient } from '@tanstack/react-query';
import { RotateCcw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { DialogFooter } from '@/components/ui/dialog';

import { SchemaFields } from './SchemaFields';
import type { ConfigScope, JsonSchema } from './types';
import { deepMerge, extractDefaults, validateAll } from './utils';

export interface ConfigFormProps {
  schema: ConfigSchemaResponse;
  open: boolean;
  pluginId?: string;
  scope?: ConfigScope;
  getConfig: () => Promise<Record<string, unknown>>;
  putConfig: (config: Record<string, unknown>) => Promise<{ error?: unknown }>;
  deleteConfig: () => Promise<{ error?: unknown }>;
  invalidateQueryKeys?: string[][];
}

export function ConfigForm({
  schema,
  open,
  pluginId,
  scope,
  getConfig,
  putConfig,
  deleteConfig,
  invalidateQueryKeys,
}: ConfigFormProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const jsonSchema = schema.json_schema as JsonSchema;
  const defaults = useMemo(() => extractDefaults(jsonSchema), [jsonSchema]);

  const [values, setValues] = useState<Record<string, unknown>>(defaults);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const [message, setMessage] = useState<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);

  useEffect(() => {
    if (!open) return;
    setMessage(null);
    setErrors({});
    setLoadingData(true);

    getConfig()
      .then((config) => {
        setValues(deepMerge(defaults, config));
      })
      .catch((err) => {
        if (err?.code !== 'NOT_FOUND') {
          setMessage({ type: 'error', text: t('plugins.config.loadError') });
        }
        setValues(defaults);
      })
      .finally(() => setLoadingData(false));
  }, [open, schema.namespace]); // eslint-disable-line react-hooks/exhaustive-deps

  const updateValue = useCallback((path: string[], value: unknown) => {
    setValues((prev) => {
      if (path.length === 0) return prev;
      const next = structuredClone(prev);
      let target: Record<string, unknown> = next;
      for (let i = 0; i < path.length - 1; i++) {
        if (
          target[path[i]] === undefined ||
          typeof target[path[i]] !== 'object'
        ) {
          // Create array or object depending on whether next segment is numeric
          target[path[i]] = /^\d+$/.test(path[i + 1]) ? [] : {};
        }
        target = target[path[i]] as Record<string, unknown>;
      }
      target[path[path.length - 1]] = value;
      return next;
    });

    const dotPath = path.join('.');
    setErrors((prev) => {
      if (!prev[dotPath]) return prev;
      const next = { ...prev };
      delete next[dotPath];
      return next;
    });
  }, []);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();

    const validationErrors = validateAll(values, jsonSchema);
    if (Object.keys(validationErrors).length > 0) {
      setErrors(validationErrors);
      return;
    }

    setLoading(true);
    setMessage(null);

    const { error } = await putConfig(values);

    setLoading(false);
    if (error) {
      setMessage({ type: 'error', text: t('plugins.config.saveError') });
    } else {
      setMessage({ type: 'success', text: t('plugins.config.saveSuccess') });
      if (invalidateQueryKeys) {
        for (const key of invalidateQueryKeys) {
          queryClient.invalidateQueries({ queryKey: key });
        }
      }
    }
  }

  function handleReset() {
    setValues(defaults);
    setErrors({});
    setMessage(null);
  }

  async function handleDelete() {
    setLoading(true);
    setMessage(null);

    const { error } = await deleteConfig();

    setLoading(false);
    if (error) {
      setMessage({ type: 'error', text: t('plugins.config.deleteError') });
    } else {
      setValues(defaults);
      setErrors({});
      setMessage({
        type: 'success',
        text: t('plugins.config.deleteSuccess'),
      });
      if (invalidateQueryKeys) {
        for (const key of invalidateQueryKeys) {
          queryClient.invalidateQueries({ queryKey: key });
        }
      }
    }
  }

  if (loadingData) {
    return (
      <div className="py-12 text-center text-muted-foreground text-sm">
        {t('plugins.config.loading')}
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      {schema.description && (
        <p className="text-sm text-muted-foreground leading-relaxed">
          {schema.description}
        </p>
      )}

      {pluginId && (
        <Slot
          name={`config.form.${pluginId}.${schema.namespace}`}
          as="div"
          slotProps={{
            scope,
            pluginId,
            namespace: schema.namespace,
            values,
            schema: jsonSchema,
          }}
        />
      )}

      <div className="space-y-5">
        <SchemaFields
          schema={jsonSchema}
          values={values}
          path={[]}
          updateValue={updateValue}
          errors={errors}
          pluginId={pluginId}
          namespace={schema.namespace}
          scope={scope}
        />
      </div>

      {message && (
        <div
          className={`rounded-lg px-4 py-3 text-sm ${
            message.type === 'success'
              ? 'bg-green-500/10 text-green-600 dark:text-green-400 border border-green-500/20'
              : 'bg-destructive/10 text-destructive border border-destructive/20'
          }`}
        >
          {message.text}
        </div>
      )}

      <DialogFooter className="gap-2 sm:gap-0 pt-2">
        <div className="flex gap-2 mr-auto">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleReset}
            disabled={loading}
          >
            <RotateCcw className="h-3.5 w-3.5 mr-1.5" />
            {t('plugins.config.defaults')}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleDelete}
            disabled={loading}
            className="text-destructive hover:text-destructive"
          >
            <Trash2 className="h-3.5 w-3.5 mr-1.5" />
            {t('plugins.config.delete')}
          </Button>
        </div>
        <Button type="submit" disabled={loading}>
          {loading ? t('plugins.config.saving') : t('plugins.config.save')}
        </Button>
      </DialogFooter>
    </form>
  );
}
