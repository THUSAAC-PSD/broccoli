import type { ConfigSchemaResponse, PluginDetailResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQueryClient } from '@tanstack/react-query';
import {
  ChevronDown,
  ChevronUp,
  Minus,
  Plus,
  RotateCcw,
  Trash2,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

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
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface PluginConfigDialogProps {
  plugin: PluginDetailResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

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
  multipleOf?: number;
  'x-precision'?: number;
  'x-unit'?: string;
}

interface JsonSchema extends JsonSchemaProperty {
  type: 'object';
  properties?: Record<string, JsonSchemaProperty>;
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

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

/** Validate a single value against its schema property. Returns error string or null. */
function validateField(
  value: unknown,
  prop: JsonSchemaProperty,
): string | null {
  if (value === undefined || value === null) return null;

  if (prop.type === 'number' || prop.type === 'integer') {
    if (typeof value !== 'number' || Number.isNaN(value)) {
      return 'Must be a valid number';
    }
    if (prop.type === 'integer' && !Number.isInteger(value)) {
      return 'Must be a whole number';
    }
    if (prop.minimum !== undefined && value < prop.minimum) {
      return `Must be at least ${String(prop.minimum)}`;
    }
    if (prop.maximum !== undefined && value > prop.maximum) {
      return `Must be at most ${String(prop.maximum)}`;
    }
  }

  if (prop.type === 'string' && typeof value === 'string') {
    if (prop.minLength !== undefined && value.length < prop.minLength) {
      return `Must be at least ${String(prop.minLength)} characters`;
    }
    if (prop.maxLength !== undefined && value.length > prop.maxLength) {
      return `Must be at most ${String(prop.maxLength)} characters`;
    }
  }

  return null;
}

/** Recursively validate all fields. Returns a map of dotted-path → error. */
function validateAll(
  values: Record<string, unknown>,
  schema: JsonSchema,
  prefix: string[] = [],
): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!schema.properties) return errors;

  for (const [key, prop] of Object.entries(schema.properties)) {
    const path = [...prefix, key];
    const dotPath = path.join('.');
    const value = values[key];

    if (prop.type === 'object' && prop.properties) {
      const nested = validateAll(
        (value && typeof value === 'object' ? value : {}) as Record<
          string,
          unknown
        >,
        prop as JsonSchema,
        path,
      );
      Object.assign(errors, nested);
    } else {
      const err = validateField(value, prop);
      if (err) errors[dotPath] = err;
    }
  }
  return errors;
}

// ---------------------------------------------------------------------------
// NumericInput
// ---------------------------------------------------------------------------

function NumericInput({
  value,
  onChange,
  min,
  max,
  integer,
  step: stepProp,
  precision,
  unit,
  id,
}: Readonly<{
  value: number | undefined;
  onChange: (v: number | undefined) => void;
  min?: number;
  max?: number;
  integer?: boolean;
  step?: number;
  precision?: number;
  unit?: string;
  id?: string;
}>) {
  const [text, setText] = useState(value !== undefined ? String(value) : '');
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Sync external value → text when not focused, applying precision formatting
  useEffect(() => {
    if (!focused) {
      if (value !== undefined) {
        setText(
          precision !== undefined ? value.toFixed(precision) : String(value),
        );
      } else {
        setText('');
      }
    }
  }, [value, focused, precision]);

  const step = stepProp ?? (integer ? 1 : 0.1);

  function clamp(n: number): number {
    let v = n;
    if (min !== undefined) v = Math.max(min, v);
    if (max !== undefined) v = Math.min(max, v);
    return v;
  }

  function commit(raw: string) {
    const trimmed = raw.trim();
    if (trimmed === '' || trimmed === '-') {
      onChange(undefined);
      setText('');
      return;
    }
    const parsed = integer
      ? Number.parseInt(trimmed, 10)
      : Number.parseFloat(trimmed);
    if (Number.isNaN(parsed)) {
      // Reset to previous valid value
      setText(value !== undefined ? String(value) : '');
      return;
    }
    const clamped = clamp(parsed);
    const final = integer ? Math.round(clamped) : clamped;
    onChange(final);
    setText(precision !== undefined ? final.toFixed(precision) : String(final));
  }

  function increment(delta: number) {
    const base = value ?? 0;
    const next = clamp(
      integer
        ? Math.round(base + delta)
        : Math.round((base + delta) * 1e10) / 1e10,
    );
    onChange(next);
    setText(precision !== undefined ? next.toFixed(precision) : String(next));
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      increment(step);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      increment(-step);
    }
  }

  // Allow digits, minus, decimal point (for floats), and 'e' for scientific notation
  function handleChange(e: React.ChangeEvent<HTMLInputElement>) {
    const raw = e.target.value;
    if (integer) {
      if (raw === '' || raw === '-' || /^-?\d*$/.test(raw)) {
        setText(raw);
      }
    } else {
      if (
        raw === '' ||
        raw === '-' ||
        raw === '.' ||
        /^-?\d*\.?\d*(?:[eE][-+]?\d*)?$/.test(raw)
      ) {
        setText(raw);
      }
    }
  }

  const rangeHint =
    min !== undefined && max !== undefined
      ? `${String(min)} – ${String(max)}`
      : min !== undefined
        ? `≥ ${String(min)}`
        : max !== undefined
          ? `≤ ${String(max)}`
          : null;

  return (
    <div className="flex items-center gap-0">
      <div className="relative flex-1">
        <input
          ref={inputRef}
          id={id}
          type="text"
          inputMode={integer ? 'numeric' : 'decimal'}
          value={text}
          onChange={handleChange}
          onBlur={(e) => {
            setFocused(false);
            commit(e.target.value);
          }}
          onFocus={() => setFocused(true)}
          onKeyDown={handleKeyDown}
          className="flex h-9 w-full rounded-l-md border border-r-0 border-input bg-transparent px-3 py-1 text-sm tabular-nums shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:z-10 disabled:cursor-not-allowed disabled:opacity-50"
          placeholder={integer ? '0' : '0.0'}
        />
        {(rangeHint || unit) && (
          <span className="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground/60 pointer-events-none select-none tabular-nums">
            {unit && rangeHint
              ? `${rangeHint} ${unit}`
              : unit
                ? unit
                : rangeHint}
          </span>
        )}
      </div>
      <div className="flex flex-col">
        <button
          type="button"
          tabIndex={-1}
          onClick={() => increment(step)}
          className="flex h-[18px] w-7 items-center justify-center rounded-tr-md border border-input bg-muted/50 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground active:bg-accent/80"
        >
          <ChevronUp className="h-3 w-3" />
        </button>
        <button
          type="button"
          tabIndex={-1}
          onClick={() => increment(-step)}
          className="flex h-[18px] w-7 items-center justify-center rounded-br-md border border-t-0 border-input bg-muted/50 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground active:bg-accent/80"
        >
          <ChevronDown className="h-3 w-3" />
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

export function PluginConfigDialog({
  plugin,
  open,
  onOpenChange,
}: PluginConfigDialogProps) {
  const schemas = plugin.config_schemas;
  const [activeTab, setActiveTab] = useState(schemas[0]?.namespace ?? '');
  const { t } = useTranslation();

  useEffect(() => {
    if (open && schemas.length > 0) {
      setActiveTab(schemas[0].namespace);
    }
  }, [open, schemas]);

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

// ---------------------------------------------------------------------------
// Form
// ---------------------------------------------------------------------------

function NamespaceConfigForm({
  pluginId,
  schema,
  open,
}: Readonly<{
  pluginId: string;
  schema: ConfigSchemaResponse;
  open: boolean;
}>) {
  const apiClient = useApiClient();
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

    apiClient
      .GET('/admin/plugins/{id}/config/{namespace}', {
        params: { path: { id: pluginId, namespace: schema.namespace } },
      })
      .then(({ data, error }) => {
        setLoadingData(false);
        if (error) {
          setMessage({ type: 'error', text: t('plugins.config.loadError') });
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
    // Clear error for this field when value changes
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
    setErrors({});
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
      setErrors({});
      setMessage({
        type: 'success',
        text: t('plugins.config.deleteSuccess'),
      });
      queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
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

      <div className="space-y-5">
        <SchemaFields
          schema={jsonSchema}
          values={values}
          path={[]}
          updateValue={updateValue}
          errors={errors}
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

// ---------------------------------------------------------------------------
// Schema field renderers
// ---------------------------------------------------------------------------

function SchemaFields({
  schema,
  values,
  path,
  updateValue,
  errors,
}: Readonly<{
  schema: JsonSchema;
  values: Record<string, unknown>;
  path: string[];
  updateValue: (path: string[], value: unknown) => void;
  errors: Record<string, string>;
}>) {
  if (!schema.properties) return null;

  // Separate top-level scalars from object groups for better layout
  const entries = Object.entries(schema.properties);
  const scalars = entries.filter(
    ([, prop]) => !(prop.type === 'object' && prop.properties),
  );
  const objects = entries.filter(
    ([, prop]) => prop.type === 'object' && prop.properties,
  );

  return (
    <div className="space-y-5">
      {/* Scalar fields in a grid for compact layout */}
      {scalars.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-4">
          {scalars.map(([key, prop]) => (
            <SchemaField
              key={key}
              name={key}
              prop={prop}
              value={values[key]}
              path={[...path, key]}
              updateValue={updateValue}
              errors={errors}
            />
          ))}
        </div>
      )}

      {/* Object sections rendered full-width */}
      {objects.map(([key, prop]) => (
        <SchemaField
          key={key}
          name={key}
          prop={prop}
          value={values[key]}
          path={[...path, key]}
          updateValue={updateValue}
          errors={errors}
        />
      ))}
    </div>
  );
}

function FieldError({ message }: Readonly<{ message?: string }>) {
  if (!message) return null;
  return <p className="text-xs text-destructive mt-1">{message}</p>;
}

function SchemaField({
  name,
  prop,
  value,
  path,
  updateValue,
  errors,
}: Readonly<{
  name: string;
  prop: JsonSchemaProperty;
  value: unknown;
  path: string[];
  updateValue: (path: string[], value: unknown) => void;
  errors: Record<string, string>;
}>) {
  const { t } = useTranslation();
  const fieldId = `cfg-${path.join('-')}`;
  const label = prop.title ?? name;
  const dotPath = path.join('.');
  const error = errors[dotPath];

  // Object → card-like grouped section
  if (prop.type === 'object' && prop.properties) {
    const objValue =
      value && typeof value === 'object'
        ? (value as Record<string, unknown>)
        : {};

    return (
      <div className="rounded-lg border bg-muted/30">
        <div className="px-4 py-3 border-b bg-muted/40 rounded-t-lg">
          <h4 className="text-sm font-medium">{label}</h4>
          {prop.description && (
            <p className="text-xs text-muted-foreground mt-0.5 leading-relaxed">
              {prop.description}
            </p>
          )}
        </div>
        <div className="p-4">
          <SchemaFields
            schema={prop as JsonSchema}
            values={objValue}
            path={path}
            updateValue={updateValue}
            errors={errors}
          />
        </div>
      </div>
    );
  }

  // Boolean → switch in a bordered row
  if (prop.type === 'boolean') {
    return (
      <div className="flex items-center justify-between rounded-lg border p-3 sm:col-span-2">
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

  // String enum → styled select
  if (prop.type === 'string' && prop.enum) {
    return (
      <div className="space-y-1.5">
        <Label
          htmlFor={fieldId}
          className="text-xs font-medium text-muted-foreground uppercase tracking-wider"
        >
          {label}
        </Label>
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
        <FieldError message={error} />
      </div>
    );
  }

  // Number / integer → custom NumericInput
  if (prop.type === 'number' || prop.type === 'integer') {
    return (
      <div className="space-y-1.5">
        <Label
          htmlFor={fieldId}
          className="text-xs font-medium text-muted-foreground uppercase tracking-wider"
        >
          {label}
        </Label>
        {prop.description && (
          <p className="text-xs text-muted-foreground">{prop.description}</p>
        )}
        <NumericInput
          id={fieldId}
          value={typeof value === 'number' ? value : undefined}
          onChange={(v) => updateValue(path, v)}
          min={prop.minimum}
          max={prop.maximum}
          integer={prop.type === 'integer'}
          step={prop.multipleOf}
          precision={prop['x-precision']}
          unit={prop['x-unit']}
        />
        <FieldError message={error} />
      </div>
    );
  }

  // Array of strings → multi-input
  if (prop.type === 'array' && prop.items?.type === 'string') {
    const items = Array.isArray(value) ? (value as string[]) : [];

    return (
      <div className="space-y-1.5 sm:col-span-2">
        <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
          {label}
        </Label>
        {prop.description && (
          <p className="text-xs text-muted-foreground">{prop.description}</p>
        )}
        <div className="space-y-1.5">
          {items.map((item, i) => (
            <div key={String(i)} className="flex gap-1.5">
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
                variant="ghost"
                size="icon"
                className="shrink-0 h-9 w-9 text-muted-foreground hover:text-destructive"
                onClick={() => {
                  updateValue(
                    path,
                    items.filter((_, j) => j !== i),
                  );
                }}
              >
                <Minus className="h-3.5 w-3.5" />
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="mt-1"
            onClick={() => updateValue(path, [...items, ''])}
          >
            <Plus className="h-3 w-3 mr-1.5" />
            {t('plugins.config.addItem')}
          </Button>
        </div>
        <FieldError message={error} />
      </div>
    );
  }

  // Default: string input
  return (
    <div className="space-y-1.5">
      <Label
        htmlFor={fieldId}
        className="text-xs font-medium text-muted-foreground uppercase tracking-wider"
      >
        {label}
      </Label>
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
      <FieldError message={error} />
    </div>
  );
}
