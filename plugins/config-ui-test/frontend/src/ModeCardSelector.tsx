/**
 * Replaces the `mode` dropdown with a visual card selector.
 * Each mode gets a card with an icon, name, and description.
 *
 * Receives slot props: { value, schema, onChange, path }
 */

import { cn } from '@broccoli/web-sdk/utils';

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
    <div className="flex flex-col gap-1.5 col-span-2">
      <label className="text-[11px] font-medium uppercase tracking-wide opacity-60">
        {schema.title ?? 'Mode'}
        <span className="ml-1.5 text-[10px] font-normal normal-case tracking-normal text-amber-600">
          (plugin override)
        </span>
      </label>
      {schema.description && (
        <p className="text-xs opacity-60 m-0">{schema.description}</p>
      )}
      <div className="grid grid-cols-2 gap-2">
        {modes.map((mode) => {
          const info = MODE_INFO[mode] ?? { icon: '\u2753', description: mode };
          const isSelected = selected === mode;
          return (
            <button
              key={mode}
              type="button"
              onClick={() => onChange(mode)}
              className={cn(
                'flex items-start gap-3 rounded-lg p-3 text-left cursor-pointer transition-colors duration-150',
                isSelected ? 'border-2 border-primary' : 'border border-input',
              )}
              style={
                isSelected
                  ? {
                      background:
                        'color-mix(in srgb, var(--primary, #4f46e5) 5%, transparent)',
                    }
                  : undefined
              }
            >
              <span className="text-xl leading-none mt-0.5">{info.icon}</span>
              <div>
                <div className="text-[13px] font-medium capitalize">{mode}</div>
                <div className="text-[11px] opacity-60 mt-0.5 leading-snug">
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
