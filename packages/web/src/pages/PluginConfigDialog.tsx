import type { ConfigSchemaResponse, PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQueryClient } from '@tanstack/react-query';
import { RotateCcw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface PluginConfigDialogProps {
  plugin: PluginDetailResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// JSON Schema property shape (subset we support)
interface JsonSchemaProperty {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  enum?: unknown[];
  items?: JsonSchemaProperty;
  properties?: Record<string, JsonSchemaProperty>;
  required?: string[];
}

interface JsonSchema extends JsonSchemaProperty {
  type: 'object';
  properties?: Record<string, JsonSchemaProperty>;
}

/** Extract all default values from a JSON Schema into a flat config object. */
function extractDefaults(schema: JsonSchema): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  if (!schema.properties) return result;
  for (const [key, prop] of Object.entries(schema.properties)) {
    if (prop.default !== undefined) {
      result[key] = prop.default;
    } else if (prop.type === 'object' && prop.properties) {
      result[key] = extractDefaults(prop as JsonSchema);
    }
  }
  return result;
}

/** Deep merge two objects, recursing into nested plain objects.
 *  Arrays and non-object values in `override` replace `base` entirely. */
function deepMerge(
  base: Record<string, unknown>,
  override: Record<string, unknown>,
): Record<string, unknown> {
  const result = { ...base };
  for (const [key, val] of Object.entries(override)) {
    if (
      val &&
      typeof val === 'object' &&
      !Array.isArray(val) &&
      result[key] &&
      typeof result[key] === 'object' &&
      !Array.isArray(result[key])
    ) {
      result[key] = deepMerge(
        result[key] as Record<string, unknown>,
        val as Record<string, unknown>,
      );
    } else {
      result[key] = val;
    }
  }
  return result;
}

export function PluginConfigDialog({
  plugin,
  open,
  onOpenChange,
}: PluginConfigDialogProps) {
  const schemas = plugin.config_schemas;
  const [activeTab, setActiveTab] = useState(schemas[0]?.namespace ?? '');

  useEffect(() => {
    if (open && schemas.length > 0) {
      setActiveTab(schemas[0].namespace);
    }
  }, [open, schemas]);

  const { t } = useTranslation();

  if (schemas.length === 0) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {t('plugins.config.title', { name: plugin.name })}
          </DialogTitle>
          <DialogDescription>
            {t('plugins.config.description')}
          </DialogDescription>
        </DialogHeader>

        {schemas.length === 1 ? (
          <NamespaceConfigForm
            pluginId={plugin.id}
            schema={schemas[0]}
            open={open}
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
                <NamespaceConfigForm
                  pluginId={plugin.id}
                  schema={s}
                  open={open && activeTab === s.namespace}
                />
              </TabsContent>
            ))}
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}

function NamespaceConfigForm({
  pluginId,
  schema,
  open,
}: {
  pluginId: string;
  schema: ConfigSchemaResponse;
  open: boolean;
}) {
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const jsonSchema = schema.json_schema as JsonSchema;
  const defaults = useMemo(() => extractDefaults(jsonSchema), [jsonSchema]);

  const [values, setValues] = useState<Record<string, unknown>>(defaults);
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const [message, setMessage] = useState<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);

  useEffect(() => {
    if (!open) return;
    setMessage(null);
    setLoadingData(true);

    apiClient
      .GET('/admin/plugins/{id}/config/{namespace}', {
        params: { path: { id: pluginId, namespace: schema.namespace } },
      })
      .then(({ data, error, response }) => {
        setLoadingData(false);
        if (error) {
          if (response?.status !== 404) {
            setMessage({ type: 'error', text: t('plugins.config.loadError') });
          }
          setValues(defaults);
          return;
        }
        setValues(
          deepMerge(defaults, (data.config ?? {}) as Record<string, unknown>),
        );
      });
  }, [open, pluginId, schema.namespace]); // eslint-disable-line react-hooks/exhaustive-deps

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
          target[path[i]] = {};
        }
        target = target[path[i]] as Record<string, unknown>;
      }
      target[path[path.length - 1]] = value;
      return next;
    });
  }, []);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setMessage(null);

    const { error } = await apiClient.PUT(
      '/admin/plugins/{id}/config/{namespace}',
      {
        params: { path: { id: pluginId, namespace: schema.namespace } },
        body: { config: values },
      },
    );

    setLoading(false);
    if (error) {
      setMessage({ type: 'error', text: t('plugins.config.saveError') });
    } else {
      setMessage({ type: 'success', text: t('plugins.config.saveSuccess') });
      queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
    }
  }

  function handleReset() {
    setValues(defaults);
    setMessage(null);
  }

  async function handleDelete() {
    setLoading(true);
    setMessage(null);

    const { error } = await apiClient.DELETE(
      '/admin/plugins/{id}/config/{namespace}',
      {
        params: { path: { id: pluginId, namespace: schema.namespace } },
      },
    );

    setLoading(false);
    if (error) {
      setMessage({ type: 'error', text: t('plugins.config.deleteError') });
    } else {
      setValues(defaults);
      setMessage({
        type: 'success',
        text: t('plugins.config.deleteSuccess'),
      });
      queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
    }
  }

  if (loadingData) {
    return (
      <div className="py-8 text-center text-muted-foreground">
        {t('plugins.config.loading')}
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {schema.description && (
        <p className="text-sm text-muted-foreground">{schema.description}</p>
      )}

      <SchemaFields
        schema={jsonSchema}
        values={values}
        path={[]}
        updateValue={updateValue}
      />

      {message && (
        <div
          className={`rounded-md px-4 py-3 text-sm ${
            message.type === 'success'
              ? 'bg-green-500/10 text-green-500 border border-green-500/20'
              : 'bg-destructive/10 text-destructive border border-destructive/20'
          }`}
        >
          {message.text}
        </div>
      )}

      <DialogFooter className="gap-2 sm:gap-0">
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

/** Recursively render form fields from a JSON Schema. */
function SchemaFields({
  schema,
  values,
  path,
  updateValue,
}: Readonly<{
  schema: JsonSchema;
  values: Record<string, unknown>;
  path: string[];
  updateValue: (path: string[], value: unknown) => void;
}>) {
  if (!schema.properties) return null;

  return (
    <div className="space-y-4">
      {Object.entries(schema.properties).map(([key, prop]) => (
        <SchemaField
          key={key}
          name={key}
          prop={prop}
          value={values[key]}
          path={[...path, key]}
          updateValue={updateValue}
        />
      ))}
    </div>
  );
}

function SchemaField({
  name,
  prop,
  value,
  path,
  updateValue,
}: Readonly<{
  name: string;
  prop: JsonSchemaProperty;
  value: unknown;
  path: string[];
  updateValue: (path: string[], value: unknown) => void;
}>) {
  const { t } = useTranslation();
  const fieldId = `cfg-${path.join('-')}`;
  const label = prop.title ?? name;

  // Object → render grouped section with recursive fields
  if (prop.type === 'object' && prop.properties) {
    const objValue =
      value && typeof value === 'object'
        ? (value as Record<string, unknown>)
        : {};

    return (
      <div className="space-y-3">
        <Separator />
        <div>
          <Label className="text-sm font-medium">{label}</Label>
          {prop.description && (
            <p className="text-xs text-muted-foreground mt-0.5">
              {prop.description}
            </p>
          )}
        </div>
        <div className="pl-4 border-l-2 border-muted space-y-4">
          <SchemaFields
            schema={prop as JsonSchema}
            values={objValue}
            path={path}
            updateValue={updateValue}
          />
        </div>
      </div>
    );
  }

  // Boolean → switch
  if (prop.type === 'boolean') {
    return (
      <div className="flex items-center justify-between rounded-lg border p-3">
        <div>
          <Label htmlFor={fieldId} className="cursor-pointer">
            {label}
          </Label>
          {prop.description && (
            <p className="text-xs text-muted-foreground mt-0.5">
              {prop.description}
            </p>
          )}
        </div>
        <Switch
          id={fieldId}
          checked={Boolean(value)}
          onCheckedChange={(v) => updateValue(path, v)}
        />
      </div>
    );
  }

  // String with enum → native select
  if (prop.type === 'string' && prop.enum) {
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{label}</Label>
        {prop.description && (
          <p className="text-xs text-muted-foreground">{prop.description}</p>
        )}
        <select
          id={fieldId}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => updateValue(path, e.target.value)}
          className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          {prop.enum.map((opt) => (
            <option key={String(opt)} value={String(opt)}>
              {String(opt)}
            </option>
          ))}
        </select>
      </div>
    );
  }

  // Number / integer
  if (prop.type === 'number' || prop.type === 'integer') {
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{label}</Label>
        {prop.description && (
          <p className="text-xs text-muted-foreground">{prop.description}</p>
        )}
        <Input
          id={fieldId}
          type="number"
          step={prop.type === 'integer' ? 1 : 'any'}
          min={prop.minimum}
          max={prop.maximum}
          value={value !== undefined ? String(value) : ''}
          onChange={(e) => {
            const v = e.target.value;
            if (v === '') {
              updateValue(path, undefined);
            } else {
              const parsed =
                prop.type === 'integer'
                  ? Number.parseInt(v, 10)
                  : Number.parseFloat(v);
              updateValue(path, Number.isNaN(parsed) ? undefined : parsed);
            }
          }}
        />
      </div>
    );
  }

  // Array of strings → multi-input
  if (prop.type === 'array' && prop.items?.type === 'string') {
    const items = Array.isArray(value) ? (value as string[]) : [];

    return (
      <div className="space-y-2">
        <Label>{label}</Label>
        {prop.description && (
          <p className="text-xs text-muted-foreground">{prop.description}</p>
        )}
        <div className="space-y-1.5">
          {items.map((item, i) => (
            <div key={String(i)} className="flex gap-2">
              <Input
                value={item}
                onChange={(e) => {
                  const next = [...items];
                  next[i] = e.target.value;
                  updateValue(path, next);
                }}
                className="font-mono text-sm"
              />
              <Button
                type="button"
                variant="outline"
                size="icon"
                className="shrink-0 h-9 w-9 text-destructive hover:text-destructive"
                onClick={() => {
                  updateValue(
                    path,
                    items.filter((_, j) => j !== i),
                  );
                }}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => updateValue(path, [...items, ''])}
          >
            {t('plugins.config.addItem')}
          </Button>
        </div>
      </div>
    );
  }

  // Default: string input
  return (
    <div className="space-y-2">
      <Label htmlFor={fieldId}>{label}</Label>
      {prop.description && (
        <p className="text-xs text-muted-foreground">{prop.description}</p>
      )}
      <Input
        id={fieldId}
        value={typeof value === 'string' ? value : ''}
        onChange={(e) => updateValue(path, e.target.value)}
        minLength={prop.minLength}
        maxLength={prop.maxLength}
      />
    </div>
  );
}
