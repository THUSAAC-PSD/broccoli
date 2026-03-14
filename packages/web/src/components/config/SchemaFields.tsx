import { SchemaField } from './SchemaField';
import type { ConfigScope, JsonSchema } from './types';

export function SchemaFields({
  schema,
  values,
  rootValues,
  path,
  updateValue,
  errors,
  pluginId,
  namespace,
  scope,
  isExplicitValue,
  hasExplicitDescendant,
}: Readonly<{
  schema: JsonSchema;
  values: Record<string, unknown>;
  rootValues: Record<string, unknown>;
  path: string[];
  updateValue: (path: string[], value: unknown) => void;
  errors: Record<string, string>;
  pluginId?: string;
  namespace?: string;
  scope?: ConfigScope;
  isExplicitValue: (path: string[]) => boolean;
  hasExplicitDescendant: (path: string[]) => boolean;
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
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-5">
          {scalars.map(([key, prop]) => (
            <SchemaField
              key={key}
              name={key}
              prop={prop}
              value={values[key]}
              rootValues={rootValues}
              path={[...path, key]}
              updateValue={updateValue}
              errors={errors}
              pluginId={pluginId}
              namespace={namespace}
              scope={scope}
              isExplicitValue={isExplicitValue}
              hasExplicitDescendant={hasExplicitDescendant}
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
          rootValues={rootValues}
          path={[...path, key]}
          updateValue={updateValue}
          errors={errors}
          pluginId={pluginId}
          namespace={namespace}
          scope={scope}
          isExplicitValue={isExplicitValue}
          hasExplicitDescendant={hasExplicitDescendant}
        />
      ))}
    </div>
  );
}
