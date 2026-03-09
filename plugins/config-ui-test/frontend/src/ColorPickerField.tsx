/**
 * Replaces the plain text input for `accent_color` with a native color picker
 * plus a text input showing the hex value, and a live preview swatch.
 *
 * Receives slot props: { value, schema, onChange, path }
 */

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
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '6px',
        gridColumn: 'span 2',
      }}
    >
      <label
        style={{
          fontSize: '11px',
          fontWeight: 500,
          textTransform: 'uppercase',
          letterSpacing: '0.05em',
          opacity: 0.6,
        }}
      >
        {schema.title ?? 'Color'}
        <span
          style={{
            marginLeft: '6px',
            fontSize: '10px',
            fontWeight: 400,
            textTransform: 'none',
            letterSpacing: 'normal',
            color: '#d97706',
          }}
        >
          (plugin override)
        </span>
      </label>
      {schema.description && (
        <p style={{ fontSize: '12px', opacity: 0.6, margin: 0 }}>
          {schema.description}
        </p>
      )}
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        {/* Native color picker */}
        <input
          type="color"
          value={color}
          onChange={(e) => onChange(e.target.value)}
          style={{
            height: '36px',
            width: '48px',
            cursor: 'pointer',
            borderRadius: '6px',
            border: '1px solid var(--border, #e5e7eb)',
            padding: '2px',
          }}
        />
        {/* Hex text input */}
        <input
          type="text"
          value={color}
          onChange={(e) => onChange(e.target.value)}
          maxLength={20}
          style={{
            height: '36px',
            width: '112px',
            borderRadius: '6px',
            border: '1px solid var(--border, #e5e7eb)',
            background: 'transparent',
            padding: '4px 12px',
            fontSize: '13px',
            fontFamily: 'monospace',
          }}
        />
        {/* Live preview swatch */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <div
            style={{
              height: '36px',
              width: '36px',
              borderRadius: '6px',
              border: '1px solid var(--border, #e5e7eb)',
              backgroundColor: color,
            }}
          />
          <span style={{ fontSize: '12px', opacity: 0.6 }}>Preview</span>
        </div>
      </div>
    </div>
  );
}
