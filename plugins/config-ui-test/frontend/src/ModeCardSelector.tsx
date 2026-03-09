/**
 * Replaces the `mode` dropdown with a visual card selector.
 * Each mode gets a card with an icon, name, and description.
 *
 * Receives slot props: { value, schema, onChange, path }
 */

interface ModeCardSelectorProps {
  value: unknown;
  schema: { title?: string; description?: string; enum?: string[] };
  onChange: (value: unknown) => void;
}

const MODE_INFO: Record<string, { icon: string; description: string }> = {
  fast: { icon: '\u26A1', description: 'Quick checks, minimal validation' },
  balanced: {
    icon: '\u2696\uFE0F',
    description: 'Good trade-off between speed and coverage',
  },
  thorough: {
    icon: '\uD83D\uDD0D',
    description: 'Full validation, slower but comprehensive',
  },
  experimental: {
    icon: '\uD83E\uDDEA',
    description: 'Bleeding-edge features, may be unstable',
  },
};

export function ModeCardSelector({
  value,
  schema,
  onChange,
}: ModeCardSelectorProps) {
  const modes = schema.enum ?? Object.keys(MODE_INFO);
  const selected = typeof value === 'string' ? value : '';

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
        {schema.title ?? 'Mode'}
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
      <div
        style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}
      >
        {modes.map((mode) => {
          const info = MODE_INFO[mode] ?? { icon: '\u2753', description: mode };
          const isSelected = selected === mode;
          return (
            <button
              key={mode}
              type="button"
              onClick={() => onChange(mode)}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: '12px',
                borderRadius: '8px',
                border: isSelected
                  ? '2px solid var(--primary, #4f46e5)'
                  : '1px solid var(--border, #e5e7eb)',
                padding: '12px',
                textAlign: 'left' as const,
                cursor: 'pointer',
                background: isSelected
                  ? 'color-mix(in srgb, var(--primary, #4f46e5) 5%, transparent)'
                  : 'transparent',
                transition: 'border-color 0.15s, background 0.15s',
              }}
            >
              <span
                style={{ fontSize: '20px', lineHeight: 1, marginTop: '2px' }}
              >
                {info.icon}
              </span>
              <div>
                <div
                  style={{
                    fontSize: '13px',
                    fontWeight: 500,
                    textTransform: 'capitalize' as const,
                  }}
                >
                  {mode}
                </div>
                <div
                  style={{
                    fontSize: '11px',
                    opacity: 0.6,
                    marginTop: '2px',
                    lineHeight: 1.4,
                  }}
                >
                  {info.description}
                </div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
