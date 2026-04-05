import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ConfigFormSlotProps } from '@broccoli/web-sdk/slot';
import React from 'react';

function buildCommand(parts: (string | undefined)[]): string {
  return parts.filter(Boolean).join(' ');
}

function buildPreviews(values: Record<string, unknown>) {
  const cpp = values.cpp as { compiler?: string; flags?: string[] } | undefined;
  const c = values.c as { compiler?: string; flags?: string[] } | undefined;
  const py = values.python3 as { interpreter?: string } | undefined;
  const java = values.java as
    | { compiler?: string; flags?: string[] }
    | undefined;

  return [
    {
      key: 'lang.cpp',
      command: buildCommand([
        cpp?.compiler,
        ...(cpp?.flags ?? []),
        '<source>',
        '-o',
        '<binary>',
      ]),
    },
    {
      key: 'lang.c',
      command: buildCommand([
        c?.compiler,
        ...(c?.flags ?? []),
        '<source>',
        '-o',
        '<binary>',
        '-lm',
      ]),
    },
    {
      key: 'lang.python3',
      command: buildCommand([py?.interpreter, '<source>']),
    },
    {
      key: 'lang.java',
      command: buildCommand([
        java?.compiler,
        ...(java?.flags ?? []),
        '<source>',
      ]),
    },
  ];
}

export function CompileInfoBanner({ values }: ConfigFormSlotProps) {
  const { t } = useTranslation();
  const previews = buildPreviews(values);

  return (
    <div className="rounded-md border border-border bg-muted/50 p-3 mb-3">
      <div className="text-xs font-medium text-muted-foreground mb-2">
        {t('lang.compileCommandPreview')}
      </div>
      <div className="space-y-1">
        {previews.map((p) => (
          <div key={p.key} className="flex items-baseline gap-2 text-xs">
            <span className="font-medium text-foreground w-16 shrink-0">
              {t(p.key)}
            </span>
            <code className="text-muted-foreground font-mono text-[11px] break-all">
              {p.command}
            </code>
          </div>
        ))}
      </div>
    </div>
  );
}
