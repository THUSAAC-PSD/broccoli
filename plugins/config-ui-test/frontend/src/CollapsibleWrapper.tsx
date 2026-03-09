/**
 * Wraps the `scoring` object section in a collapsible panel.
 * Uses the "wrap" slot position — receives children as the original field content.
 *
 * Receives: { children } (the default SchemaField rendering for scoring)
 */

import { type ReactNode, useState } from 'react';

interface CollapsibleWrapperProps {
  children: ReactNode;
}

export function CollapsibleWrapper({ children }: CollapsibleWrapperProps) {
  const [collapsed, setCollapsed] = useState(false);

  return (
    <div style={{ position: 'relative' }}>
      <div style={{ position: 'absolute', top: '-4px', right: 0, zIndex: 10 }}>
        <button
          type="button"
          onClick={() => setCollapsed((v) => !v)}
          style={{
            borderRadius: '6px',
            border: '1px solid var(--border, #e5e7eb)',
            background: 'var(--background, #fff)',
            padding: '2px 8px',
            fontSize: '11px',
            cursor: 'pointer',
            opacity: 0.7,
            transition: 'opacity 0.15s',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.opacity = '1';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.opacity = '0.7';
          }}
        >
          {collapsed ? 'Expand' : 'Collapse'}
          <span
            style={{ marginLeft: '4px', fontSize: '10px', color: '#d97706' }}
          >
            (plugin)
          </span>
        </button>
      </div>
      <div
        style={{
          transition: 'max-height 0.2s ease, opacity 0.2s ease',
          maxHeight: collapsed ? '0px' : '2000px',
          overflow: collapsed ? 'hidden' : 'visible',
          opacity: collapsed ? 0 : 1,
        }}
      >
        {children}
      </div>
      {collapsed && (
        <div
          style={{
            borderRadius: '8px',
            border:
              '1px dashed color-mix(in srgb, currentColor 30%, transparent)',
            padding: '16px',
            textAlign: 'center' as const,
            fontSize: '12px',
            opacity: 0.5,
          }}
        >
          Scoring section collapsed — click Expand to configure
        </div>
      )}
    </div>
  );
}
