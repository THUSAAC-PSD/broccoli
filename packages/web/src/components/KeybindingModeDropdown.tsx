import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@broccoli/web-sdk/ui';
import { Keyboard } from 'lucide-react';

import type { KeybindingMode } from '@/lib/use-keybinding-mode';

const MODES = ['normal', 'vim', 'emacs'] as const;

function modeLabel(mode: KeybindingMode, normalLabel: string): string {
  if (mode === 'normal') return normalLabel;
  return mode === 'vim' ? 'Vim' : 'Emacs';
}

interface KeybindingModeDropdownProps {
  mode: KeybindingMode;
  onChange: (mode: KeybindingMode) => void;
  /** Use compact styling (plain button instead of Button component). */
  compact?: boolean;
}

export function KeybindingModeDropdown({
  mode,
  onChange,
  compact,
}: KeybindingModeDropdownProps) {
  const { t } = useTranslation();
  const normalLabel = t('editor.keybindingNormal');

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        {compact ? (
          <button
            type="button"
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
            title={t('editor.keybindings')}
            aria-label={t('editor.keybindings')}
          >
            <Keyboard className="h-3 w-3" />
            {modeLabel(mode, normalLabel)}
          </button>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            title={t('editor.keybindings')}
            aria-label={t('editor.keybindings')}
            className="gap-1.5 text-muted-foreground"
          >
            <Keyboard className="h-3.5 w-3.5" />
            <span className="text-xs">{modeLabel(mode, normalLabel)}</span>
          </Button>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {MODES.map((m) => (
          <DropdownMenuItem
            key={m}
            onClick={() => onChange(m)}
            className={mode === m ? 'bg-accent' : ''}
          >
            <span className="flex items-center gap-2">
              <span
                className={`inline-block h-1.5 w-1.5 rounded-full ${mode === m ? 'bg-primary' : 'bg-transparent'}`}
              />
              {modeLabel(m, normalLabel)}
            </span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
