import type { JsonSchema, JsonSchemaProperty } from './types';

export function extractDefaults(schema: JsonSchema): Record<string, unknown> {
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

export function deepMerge(
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

export function validateField(
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

function validateItems(
  items: unknown[],
  itemSchema: JsonSchemaProperty,
  parentPath: string[],
  errors: Record<string, string>,
): void {
  for (let i = 0; i < items.length; i++) {
    const itemPath = [...parentPath, String(i)];
    const item = items[i];
    if (itemSchema.type === 'object' && itemSchema.properties) {
      Object.assign(
        errors,
        validateAll(
          (item && typeof item === 'object' ? item : {}) as Record<
            string,
            unknown
          >,
          itemSchema as JsonSchema,
          itemPath,
        ),
      );
    } else if (
      itemSchema.type === 'array' &&
      itemSchema.items &&
      Array.isArray(item)
    ) {
      validateItems(item as unknown[], itemSchema.items, itemPath, errors);
    } else {
      const err = validateField(item, itemSchema);
      if (err) errors[itemPath.join('.')] = err;
    }
  }
}

export function validateAll(
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
    } else if (prop.type === 'array' && prop.items && Array.isArray(value)) {
      validateItems(value, prop.items, path, errors);
    } else {
      const err = validateField(value, prop);
      if (err) errors[dotPath] = err;
    }
  }
  return errors;
}

export function defaultForType(prop: JsonSchemaProperty): unknown {
  if (prop.default !== undefined) return prop.default;
  switch (prop.type) {
    case 'string':
      return '';
    case 'number':
    case 'integer':
      return 0;
    case 'boolean':
      return false;
    case 'object':
      return prop.properties ? extractDefaults(prop as JsonSchema) : {};
    case 'array':
      return [];
    default:
      return '';
  }
}
