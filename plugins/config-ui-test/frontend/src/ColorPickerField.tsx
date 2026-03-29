/**
 * Replaces the plain text input for `accent_color` with a native color picker
 * plus a text input showing the hex value, and a live preview swatch.
 *
 * Receives slot props: { value, schema, onChange, path }
 */

import { Input, Label } from '@broccoli/web-sdk/ui';

interface ColorPickerFieldProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
}

export function ColorPickerField({
  value,
  schema,
  onChange,
}: ColorPickerFieldProps) {
  const color = typeof value === 'string' ? value : '#000000';

  return (
    <div className="flex flex-col gap-1.5 col-span-2">
      <Label className="text-[11px] font-medium uppercase tracking-wide opacity-60">
        {schema.title ?? 'Color'}
        <span className="ml-1.5 text-[10px] font-normal normal-case tracking-normal text-amber-600">
          (plugin override)
        </span>
      </Label>
      {schema.description && (
        <p className="text-xs opacity-60 m-0">{schema.description}</p>
      )}
      <div className="flex items-center gap-3">
        {/* Native color picker */}
        <input
          type="color"
          value={color}
          onChange={(e) => onChange(e.target.value)}
          className="h-9 w-12 cursor-pointer rounded-md border border-input p-0.5"
        />
        {/* Hex text input */}
        <Input
          type="text"
          value={color}
          onChange={(e) => onChange(e.target.value)}
          maxLength={20}
          className="w-28 font-mono"
        />
        {/* Live preview swatch */}
        <div className="flex items-center gap-2">
          <div
            className="h-9 w-9 rounded-md border border-input"
            style={{ backgroundColor: color }}
          />
          <span className="text-xs opacity-60">Preview</span>
        </div>
      </div>
    </div>
  );
}
