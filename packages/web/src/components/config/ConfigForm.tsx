import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import { Slot } from '@broccoli/web-sdk/slot';
import { Button, SheetFooter } from '@broccoli/web-sdk/ui';
import { useQueryClient } from '@tanstack/react-query';
import { RotateCcw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { SchemaFields } from './SchemaFields';
import type { ConfigScope, InheritedConfig, JsonSchema } from './types';
import {
  deepMerge,
  extractDefaults,
  hasOwnDescendantValue,
  hasOwnValueAtPath,
  resolveInheritedValue,
  validateAll,
} from './utils';

type ConfigSchemaResponse = PluginDetail['config_schemas'][number];

export interface ConfigFormProps {
  schema: ConfigSchemaResponse;
  open: boolean;
  pluginId?: string;
  scope?: ConfigScope;
  inherited?: InheritedConfig;
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
  inherited,
  getConfig,
  putConfig,
  deleteConfig,
  invalidateQueryKeys,
}: ConfigFormProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const jsonSchema = schema.json_schema as JsonSchema;
  const schemaDefaults = useMemo(
    () => extractDefaults(jsonSchema),
    [jsonSchema],
  );

  const defaults = useMemo(() => {
    if (!inherited) return schemaDefaults;
    const merged = { ...schemaDefaults };
    for (const key of Object.keys(merged)) {
      const iv = resolveInheritedValue(key, inherited);
      if (iv !== null) merged[key] = iv.value;
    }
    return merged;
  }, [schemaDefaults, inherited]);

  const [values, setValues] = useState<Record<string, unknown>>(defaults);
  const [storedValues, setStoredValues] = useState<Record<string, unknown>>({});
  const [dirtyFields, setDirtyFields] = useState<Set<string>>(new Set());
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
        setStoredValues(config);
        setValues(deepMerge(defaults, config));
      })
      .catch((err) => {
        if (err?.code !== 'NOT_FOUND') {
          setMessage({ type: 'error', text: t('plugins.config.loadError') });
        }
        setStoredValues({});
        setValues(defaults);
      })
      .finally(() => setLoadingData(false));
  }, [open, schema.namespace]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (
      !loadingData &&
      !hasOwnDescendantValue(storedValues, []) &&
      dirtyFields.size === 0
    ) {
      setValues(deepMerge(defaults, storedValues));
    }
  }, [defaults]); // eslint-disable-line react-hooks/exhaustive-deps -- only re-sync when defaults change (i.e., inherited arrived)

  const isUsingDefaultsOnly = useMemo(
    () => !hasOwnDescendantValue(storedValues, []),
    [storedValues],
  );
  const isExplicitValue = useCallback(
    (path: string[]) => hasOwnValueAtPath(storedValues, path),
    [storedValues],
  );
  const hasExplicitDescendant = useCallback(
    (path: string[]) => hasOwnDescendantValue(storedValues, path),
    [storedValues],
  );

  const isDirty = useCallback(
    (path: string[]) => dirtyFields.has(path.join('.')),
    [dirtyFields],
  );

  const updateValue = useCallback((path: string[], value: unknown) => {
    const dotPath = path.join('.');
    setDirtyFields((prev) => {
      if (prev.has(dotPath)) return prev;
      const next = new Set(prev);
      next.add(dotPath);
      return next;
    });

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

    const dirtyTopKeys = new Set<string>();
    for (const dp of dirtyFields) {
      dirtyTopKeys.add(dp.split('.')[0]);
    }
    const payload: Record<string, unknown> = {};
    for (const key of Object.keys(values)) {
      if (Object.hasOwn(storedValues, key) || dirtyTopKeys.has(key)) {
        payload[key] = values[key];
      }
    }

    const { error } = await putConfig(payload);

    setLoading(false);
    if (error) {
      setMessage({ type: 'error', text: t('plugins.config.saveError') });
    } else {
      setStoredValues(payload);
      setDirtyFields(new Set());
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
    setDirtyFields(new Set());
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
      setStoredValues({});
      setValues(defaults);
      setDirtyFields(new Set());
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
            storedValues,
            schema: jsonSchema,
            isUsingDefaultsOnly,
            inherited,
            hasExplicitValue: (path: string | string[]) =>
              isExplicitValue(Array.isArray(path) ? path : [path]),
          }}
        />
      )}

      {isUsingDefaultsOnly && (
        <div className="rounded-lg border border-dashed bg-muted/40 px-4 py-3 text-sm text-muted-foreground">
          {t('plugins.config.unsetNotice')}
        </div>
      )}

      <div className="space-y-5">
        <SchemaFields
          schema={jsonSchema}
          values={values}
          rootValues={values}
          path={[]}
          updateValue={updateValue}
          errors={errors}
          pluginId={pluginId}
          namespace={schema.namespace}
          scope={scope}
          isExplicitValue={isExplicitValue}
          hasExplicitDescendant={hasExplicitDescendant}
          isDirty={isDirty}
          inherited={inherited}
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

      <SheetFooter className="gap-2 sm:gap-0 pt-2">
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
            title={t('plugins.config.deleteHint')}
          >
            <Trash2 className="h-3.5 w-3.5 mr-1.5" />
            {t('plugins.config.delete')}
          </Button>
        </div>
        <Button type="submit" disabled={loading}>
          {loading ? t('plugins.config.saving') : t('plugins.config.save')}
        </Button>
      </SheetFooter>
    </form>
  );
}
