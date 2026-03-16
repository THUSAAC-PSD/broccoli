import './dock-theme.css';

import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button } from '@broccoli/web-sdk/ui';
import type { DockviewTheme } from 'dockview-core';
import type {
  DockviewApi,
  DockviewReadyEvent,
  IDockviewPanelProps,
} from 'dockview-react';
import { DockviewReact } from 'dockview-react';
import { Columns3, Plus, RotateCcw } from 'lucide-react';
import {
  createContext,
  use,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';

import { CodeEditorPanel } from '../CodeEditorPanel';
import { ProblemStatementPanel } from '../ProblemStatementPanel';
import { SubmissionsPanel } from '../SubmissionsPanel';
import { getDefaultLayout } from './default-layout';
import {
  PANEL_CODE_EDITOR,
  PANEL_PROBLEM_STATEMENT,
  PANEL_SUBMISSIONS,
} from './panel-registry';
import { DOCK_STORAGE_KEY, useDockLayout } from './use-dock-layout';

const broccoliTheme: DockviewTheme = {
  name: 'broccoli',
  className: 'dockview-theme-broccoli',
};

const ALL_PANEL_IDS = [
  PANEL_PROBLEM_STATEMENT,
  PANEL_CODE_EDITOR,
  PANEL_SUBMISSIONS,
] as const;

const PANEL_LABELS: Record<string, string> = {
  [PANEL_PROBLEM_STATEMENT]: 'Problem',
  [PANEL_CODE_EDITOR]: 'Code',
  [PANEL_SUBMISSIONS]: 'Submissions',
};

const panelComponents: Record<string, React.FC<IDockviewPanelProps>> = {
  [PANEL_PROBLEM_STATEMENT]: () => <ProblemStatementPanel />,
  [PANEL_CODE_EDITOR]: () => <CodeEditorPanel />,
  [PANEL_SUBMISSIONS]: () => <SubmissionsPanel />,
};

/** Shared context so Watermark can access the parent-owned API ref */
const DockApiContext =
  createContext<React.MutableRefObject<DockviewApi | null> | null>(null);

function Watermark() {
  const { t } = useTranslation();
  const apiRef = use(DockApiContext);

  const handleReset = () => {
    localStorage.removeItem(DOCK_STORAGE_KEY);
    apiRef?.current?.fromJSON(getDefaultLayout());
  };

  return (
    <div className="h-full flex flex-col items-center justify-center gap-4 text-muted-foreground">
      <Columns3 className="h-10 w-10 opacity-20" strokeWidth={1.5} />
      <div className="text-center space-y-1">
        <p className="text-sm font-medium text-foreground/60">
          {t('dock.empty', { defaultValue: 'No panels open' })}
        </p>
        <p className="text-xs">
          {t('dock.emptyHint', {
            defaultValue:
              'Reopen panels from the bar above, or reset the layout',
          })}
        </p>
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={handleReset}
        className="gap-1.5"
      >
        <RotateCcw className="h-3.5 w-3.5" />
        {t('dock.resetLayout', { defaultValue: 'Reset Layout' })}
      </Button>
    </div>
  );
}

interface ProblemDockLayoutProps {
  dockApiRef: React.MutableRefObject<DockviewApi | null>;
}

export function ProblemDockLayout({ dockApiRef }: ProblemDockLayoutProps) {
  const { t } = useTranslation();
  const [mounted, setMounted] = useState(false);
  const { setApi, restoreLayout } = useDockLayout();
  const [closedPanels, setClosedPanels] = useState<string[]>([]);
  const disposablesRef = useRef<Array<{ dispose: () => void }>>([]);

  useEffect(() => {
    setMounted(true);
    return () => {
      for (const d of disposablesRef.current) d.dispose();
      disposablesRef.current = [];
    };
  }, []);

  const syncClosedPanels = useCallback((api: DockviewApi) => {
    const openIds = new Set(api.panels.map((p) => p.id));
    setClosedPanels(ALL_PANEL_IDS.filter((id) => !openIds.has(id)));
  }, []);

  const onReady = useCallback(
    (event: DockviewReadyEvent) => {
      const api = event.api;
      setApi(api);
      dockApiRef.current = api;

      // Restore or use default layout
      const saved = restoreLayout();
      if (saved) {
        try {
          api.fromJSON(saved);
        } catch {
          api.fromJSON(getDefaultLayout());
        }
      } else {
        api.fromJSON(getDefaultLayout());
      }

      syncClosedPanels(api);

      // Register event listeners (always, not conditionally)
      for (const d of disposablesRef.current) d.dispose();
      disposablesRef.current = [
        api.onDidAddPanel(() => syncClosedPanels(api)),
        api.onDidRemovePanel(() => syncClosedPanels(api)),
      ];
    },
    [setApi, restoreLayout, dockApiRef, syncClosedPanels],
  );

  const reopenPanel = useCallback(
    (panelId: string) => {
      const api = dockApiRef.current;
      if (!api) return;
      api.addPanel({
        id: panelId,
        component: panelId,
        title: PANEL_LABELS[panelId] ?? panelId,
      });
    },
    [dockApiRef],
  );

  const handleReset = useCallback(() => {
    localStorage.removeItem(DOCK_STORAGE_KEY);
    dockApiRef.current?.fromJSON(getDefaultLayout());
  }, [dockApiRef]);

  if (!mounted) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground">
        <div className="animate-spin rounded-full h-5 w-5 border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <DockApiContext value={dockApiRef}>
      {/* ── Closed panels recovery bar ── */}
      {closedPanels.length > 0 && (
        <div className="flex-shrink-0 flex items-center gap-2 px-3 py-1.5 border-b bg-muted/30">
          <span className="text-[11px] font-medium text-muted-foreground tracking-wide uppercase">
            {t('dock.reopen', { defaultValue: 'Reopen' })}
          </span>
          <div className="flex items-center gap-1">
            {closedPanels.map((id) => (
              <button
                key={id}
                type="button"
                onClick={() => reopenPanel(id)}
                className="inline-flex items-center gap-1 h-6 px-2.5 rounded-md text-[11px] font-medium
                  bg-background text-foreground/80 border border-border/60
                  hover:bg-accent hover:text-accent-foreground hover:border-border
                  transition-all duration-150 ease-out"
              >
                <Plus className="h-3 w-3" />
                {PANEL_LABELS[id] ?? id}
              </button>
            ))}
          </div>
          <div className="flex-1" />
          <button
            type="button"
            onClick={handleReset}
            className="inline-flex items-center gap-1 h-6 px-2 rounded-md text-[11px]
              text-muted-foreground hover:text-foreground hover:bg-accent
              transition-colors duration-150"
            title={t('dock.resetLayout', { defaultValue: 'Reset Layout' })}
          >
            <RotateCcw className="h-3 w-3" />
            {t('dock.resetLayout', { defaultValue: 'Reset Layout' })}
          </button>
        </div>
      )}
      <DockviewReact
        className="flex-1"
        theme={broccoliTheme}
        components={panelComponents}
        watermarkComponent={Watermark}
        onReady={onReady}
      />
    </DockApiContext>
  );
}
